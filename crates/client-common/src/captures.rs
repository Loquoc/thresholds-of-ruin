//! Client-owned capture bytes and a bounded, sequential upload transaction.
//! This module never captures the desktop or reconstructs a client's display.
use std::time::Duration;

use sha2::{Digest, Sha256};
use tor_protocol::*;

use crate::connection::ConnectionError;
use crate::{Connection, PendingRequest, RequestCompletion};

#[derive(Clone, Debug)]
pub struct CapturedAttachment {
    context: InputContext,
    branch: BranchId,
    snapshot: SnapshotId,
    metadata: AttachmentMetadata,
    bytes: Vec<u8>,
}

impl CapturedAttachment {
    pub fn new(
        context: InputContext,
        branch: BranchId,
        snapshot: SnapshotId,
        media_type: AttachmentMediaType,
        bytes: Vec<u8>,
    ) -> Result<Self, String> {
        let limit = match media_type {
            AttachmentMediaType::Png => MAX_ATTACHMENT_BYTES,
            AttachmentMediaType::Text => MAX_TEXT_ATTACHMENT_BYTES,
        };
        if bytes.is_empty() || bytes.len() > limit {
            return Err(format!("Capture must contain 1 to {limit} bytes."));
        }
        if media_type == AttachmentMediaType::Text && std::str::from_utf8(&bytes).is_err() {
            return Err("Text captures must be valid UTF-8.".into());
        }
        let metadata = AttachmentMetadata {
            media_type,
            byte_len: bytes.len() as u32,
            sha256: Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        };
        Ok(Self {
            context,
            branch,
            snapshot,
            metadata,
            bytes,
        })
    }
}

/// Upload exactly the retained capture. Ordinary live messages continue through
/// the caller's presentation path. A failure never retries a possibly committed
/// request or reports acceptance as durable disk storage.
pub async fn upload(
    connection: &mut Connection,
    capture: CapturedAttachment,
    mut present: impl FnMut(&Connection, &ServerMessage) -> Result<(), ConnectionError>,
) -> Result<(), ConnectionError> {
    if connection.role() == AccessRole::Spectator {
        return Err("Spectator access is read-only.".into());
    }
    // Like a note editor, preserve the old picture anchor while validating the
    // current authority of the same attachment. A reset/branch change is never
    // silently upgraded into a different capture.
    let context = connection.state.input_context();
    if context.stream != capture.context.stream || connection.state.branch() != &capture.branch {
        return Err(
            "Capture belongs to an earlier attachment or branch; capture the current view again."
                .into(),
        );
    }
    let begin = Request::BeginAttachment {
        context,
        branch: capture.branch,
        anchor: Anchor::State {
            snapshot: capture.snapshot,
        },
        metadata: capture.metadata,
    };
    let response = exchange(connection, begin, &mut present).await?;
    let ServerMessage::AttachmentUpload {
        upload_id,
        next_offset: 0,
        ..
    } = response
    else {
        return Err("Invalid capture upload response.".into());
    };
    let staged: Result<(), ConnectionError> = async {
        for (index, chunk) in capture.bytes.chunks(ATTACHMENT_CHUNK_BYTES).enumerate() {
            let offset = (index * ATTACHMENT_CHUNK_BYTES) as u32;
            let response = exchange(
                connection,
                Request::AttachmentChunk {
                    upload_id: upload_id.clone(),
                    offset,
                    data: attachment_base64_encode(chunk),
                },
                &mut present,
            )
            .await?;
            if !matches!(response, ServerMessage::AttachmentUpload {
            upload_id: ref received, next_offset, ..
        } if received == &upload_id && next_offset == offset + chunk.len() as u32)
            {
                return Err("Invalid capture upload offset.".into());
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = staged {
        // No Commit has been sent, so aborting this known staging identity
        // cannot erase an accepted capture or duplicate a gameplay operation.
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            exchange(
                connection,
                Request::AbortAttachment {
                    upload_id: upload_id.clone(),
                },
                &mut present,
            ),
        )
        .await;
        return Err(error);
    }
    let response = exchange_reply(
        connection,
        Request::CommitAttachment {
            upload_id: upload_id.clone(),
        },
        &mut present,
    )
    .await?;
    if let ServerMessage::Error { message, .. } = response {
        // A correlated rejection, after stream synchronization, confirms that
        // this commit was not accepted. Release only its staging identity so a
        // new explicit capture need not wait for expiry. Abort never deletes an
        // accepted history entry; its failure must not hide the original error.
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            exchange(
                connection,
                Request::AbortAttachment { upload_id },
                &mut present,
            ),
        )
        .await;
        return Err(message.into());
    }
    if !matches!(response, ServerMessage::Ack { .. }) {
        return Err("Capture commit was not acknowledged.".into());
    }
    Ok(())
}

async fn exchange(
    connection: &mut Connection,
    request: Request,
    present: &mut impl FnMut(&Connection, &ServerMessage) -> Result<(), ConnectionError>,
) -> Result<ServerMessage, ConnectionError> {
    match exchange_reply(connection, request, present).await? {
        ServerMessage::Error { message, .. } => Err(message.into()),
        response => Ok(response),
    }
}

// Retain a confirmed server rejection separately from unknown transport,
// timeout, malformed-response and resynchronization outcomes. Callers must not
// infer that a failed exchange means a commit was rejected.
async fn exchange_reply(
    connection: &mut Connection,
    request: Request,
    present: &mut impl FnMut(&Connection, &ServerMessage) -> Result<(), ConnectionError>,
) -> Result<ServerMessage, ConnectionError> {
    tokio::time::timeout(Duration::from_secs(30), async {
        // Sending can itself wait on transport backpressure.
        let id = connection.request(request).await?;
        let mut pending = PendingRequest::new(id.clone());
        let mut reply = None;
        loop {
            let message = connection.next().await?;
            present(connection, &message)?;
            let correlated = match &message {
                ServerMessage::Ack { request_id, .. }
                | ServerMessage::AttachmentUpload { request_id, .. } => request_id == &id,
                ServerMessage::Error { request_id: Some(request_id), .. } => request_id == &id,
                _ => false,
            };
            if correlated && reply.is_none() {
                reply = Some(message.clone());
            }
            match pending.observe(connection, &message) {
                Some(RequestCompletion::Unknown) => return Err("Capture outcome is unknown after resynchronization. Inspect history before retrying.".into()),
                Some(RequestCompletion::Reply(_)) => {
                    let response = reply.take().ok_or("Capture reply was not retained.")?;
                    return Ok(response);
                }
                None => {}
            }
        }
    }).await.map_err(|_| -> ConnectionError {
        "Capture response timed out; its outcome may be unknown. Inspect history before retrying.".into()
    })?
}
