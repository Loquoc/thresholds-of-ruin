use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::PyModule,
};
use serde::{ser::SerializeMap, Serializer};
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use tor_client_common::{ClientState, ConfirmedReply};
use tor_protocol::{chart_pages::ChartKind, Action, ActorId, ProjectileChoiceQuery, ReplyContext};

use crate::{Session, Turn};

#[pyclass(frozen, module = "tor_client_bot._native")]
struct Observation {
    context: ReplyContext,
    contents: String,
}

fn encode_observation(state: &ClientState, diagnostics: bool) -> serde_json::Result<String> {
    let mut contents = Vec::new();
    let mut serializer = serde_json::Serializer::new(&mut contents);
    let mut fields = serializer.serialize_map(None)?;
    fields.serialize_entry("state", state.state())?;
    fields.serialize_entry("readiness", &state.readiness())?;
    if diagnostics {
        let memory: Vec<_> = state.map_memory().map(|cell| cell.appearance()).collect();
        fields.serialize_entry("memory", &memory)?;
        fields.serialize_entry("cursor", &state.cursor())?;
        fields.serialize_entry("branch", &state.branch())?;
        fields.serialize_entry("history", state.history())?;
        fields.serialize_entry("narration", state.narration())?;
        fields.serialize_entry("outcome", &state.outcome_status())?;
        fields.serialize_entry("travel", &state.travel())?;
    } else {
        fields.serialize_entry("stream", &state.reply_context().input.stream)?;
        fields.serialize_entry("can_resume", &state.can_resume_intention())?;
        fields.serialize_entry("can_cancel", &state.can_cancel_intention())?;
    }
    fields.serialize_entry("intentions", state.intentions())?;
    fields.serialize_entry("finished_intentions", state.finished_intentions())?;
    fields.serialize_entry("combat_updates", state.combat_updates())?;
    fields.end()?;
    Ok(String::from_utf8(contents).expect("JSON serialization produces UTF-8"))
}

#[pymethods]
impl Observation {
    /// Return a detached copy of the player's disclosed observation.
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("json")?.call_method1("loads", (&self.contents,))
    }
}

#[pyclass(name = "Session", module = "tor_client_bot._native")]
struct PythonSession {
    runtime: Runtime,
    session: Option<Session>,
}

#[pymethods]
impl PythonSession {
    #[staticmethod]
    fn connect(py: Python<'_>, address: &str, token: String, actor: u64) -> PyResult<Self> {
        let address = address.parse().map_err(value_error)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(runtime_error)?;
        let session = py
            .detach(|| runtime.block_on(Session::connect(address, token, ActorId(actor))))
            .map_err(runtime_error)?;
        Ok(Self {
            runtime,
            session: Some(session),
        })
    }

    #[pyo3(signature = (diagnostics=true))]
    fn observe(&mut self, py: Python<'_>, diagnostics: bool) -> PyResult<Observation> {
        let session = self.session.as_mut().ok_or_else(closed)?;
        py.detach(|| {
            let state = self
                .runtime
                .block_on(session.observe())
                .map_err(runtime_error)?;
            let contents = encode_observation(state, diagnostics).map_err(runtime_error)?;
            Ok(Observation {
                context: state.reply_context(),
                contents,
            })
        })
    }

    fn act<'py>(
        &mut self,
        py: Python<'py>,
        action: &Bound<'py, PyAny>,
        observation: &Observation,
    ) -> PyResult<Bound<'py, PyAny>> {
        let encoded: String = py
            .import("json")?
            .call_method1("dumps", (action,))?
            .extract()?;
        let action: Action = serde_json::from_str(&encoded).map_err(value_error)?;
        let session = self.session.as_mut().ok_or_else(closed)?;
        let turn = py
            .detach(|| {
                self.runtime
                    .block_on(session.act(&observation.context, action))
            })
            .map_err(runtime_error)?;
        turn_result(py, turn)
    }

    /// Resolve the selected ability and actor through the shared client catalog.
    fn use_ability<'py>(
        &mut self,
        py: Python<'py>,
        observation: &Observation,
        ability: &str,
        target: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let session = self.session.as_mut().ok_or_else(closed)?;
        let state = session
            .observation()
            .ok_or_else(|| runtime_error("Refresh the player view"))?;
        if state.reply_context() != observation.context {
            return turn_result(py, Turn::StaleObservation);
        }
        let target = serde_json::from_value(json!(target)).map_err(value_error)?;
        let action =
            tor_client_common::abilities::action(&state.state().observation, ability, target)
                .map_err(value_error)?;
        let turn = py
            .detach(|| {
                self.runtime
                    .block_on(session.act(&observation.context, action))
            })
            .map_err(runtime_error)?;
        turn_result(py, turn)
    }

    fn resume<'py>(
        &mut self,
        py: Python<'py>,
        observation: &Observation,
    ) -> PyResult<Bound<'py, PyAny>> {
        let session = self.session.as_mut().ok_or_else(closed)?;
        let turn = py
            .detach(|| self.runtime.block_on(session.resume(&observation.context)))
            .map_err(runtime_error)?;
        turn_result(py, turn)
    }

    fn cancel<'py>(
        &mut self,
        py: Python<'py>,
        observation: &Observation,
    ) -> PyResult<Bound<'py, PyAny>> {
        let session = self.session.as_mut().ok_or_else(closed)?;
        let turn = py
            .detach(|| self.runtime.block_on(session.cancel(&observation.context)))
            .map_err(runtime_error)?;
        turn_result(py, turn)
    }

    fn advancement<'py>(&mut self, py: Python<'py>, input: &str) -> PyResult<Bound<'py, PyAny>> {
        let session = self.session.as_mut().ok_or_else(closed)?;
        let (turn, page) = py.detach(|| {
            let turn = self
                .runtime
                .block_on(session.advancement(input))
                .map_err(runtime_error)?;
            let page = serde_json::to_value(session.advancement_page()).map_err(runtime_error)?;
            Ok::<_, PyErr>((turn, page))
        })?;
        let mut result = turn_value(turn);
        result["page"] = page;
        py.import("json")?
            .call_method1("loads", (result.to_string(),))
    }

    /// Return a read-only page, or None if the supplied observation is stale.
    fn projectile_choices<'py>(
        &mut self,
        py: Python<'py>,
        observation: &Observation,
        ability: String,
        phase: &Bound<'py, PyAny>,
        offset: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let json = py.import("json")?;
        let encoded: String = json.call_method1("dumps", (phase,))?.extract()?;
        let phase = serde_json::from_str(&encoded).map_err(value_error)?;
        let query = ProjectileChoiceQuery {
            ability,
            phase,
            offset,
        };
        let session = self.session.as_mut().ok_or_else(closed)?;
        let page = py
            .detach(|| {
                self.runtime
                    .block_on(session.projectile_choices(&observation.context, query))
            })
            .map_err(runtime_error)?;
        json.call_method1(
            "loads",
            (serde_json::to_string(&page).map_err(runtime_error)?,),
        )
    }

    /// Return a complete disclosed chart, or None when unavailable or stale.
    fn chart<'py>(
        &mut self,
        py: Python<'py>,
        observation: &Observation,
        kind: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let kind: ChartKind = serde_json::from_value(json!(kind)).map_err(value_error)?;
        let session = self.session.as_mut().ok_or_else(closed)?;
        let chart = py.detach(|| {
            let chart = self
                .runtime
                .block_on(session.chart(&observation.context, kind))
                .map_err(runtime_error)?;
            Ok::<_, PyErr>(chart.map(|chart| {
                json!({
                    "kind": chart.kind,
                    "mode": chart.mode,
                    "acquired_at": chart.acquired_at.to_string(),
                    "components": chart.components,
                    "rows": chart.rows,
                })
            }))
        })?;
        py.import("json")?.call_method1(
            "loads",
            (serde_json::to_string(&chart).map_err(runtime_error)?,),
        )
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(session) = self.session.take() {
            py.detach(|| self.runtime.block_on(session.close()))
                .map_err(runtime_error)?;
        }
        Ok(())
    }
}

fn turn_result(py: Python<'_>, turn: Turn) -> PyResult<Bound<'_, PyAny>> {
    py.import("json")?
        .call_method1("loads", (turn_value(turn).to_string(),))
}

fn turn_value(turn: Turn) -> Value {
    match turn {
        Turn::Confirmed(ConfirmedReply::Receipt(receipt)) => {
            json!({"status": "confirmed", "receipt": receipt})
        }
        Turn::Confirmed(ConfirmedReply::Rejected { code, message }) => {
            json!({"status": "rejected", "code": code, "message": message})
        }
        Turn::Confirmed(ConfirmedReply::Completed) => json!({"status": "confirmed"}),
        Turn::StaleObservation => json!({"status": "stale_observation"}),
        Turn::IntentionUnavailable => json!({"status": "intention_unavailable"}),
        Turn::UnknownOutcome => json!({"status": "unknown_outcome"}),
    }
}

fn closed() -> PyErr {
    PyRuntimeError::new_err("Session is closed")
}

fn runtime_error(error: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn value_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Observation>()?;
    module.add_class::<PythonSession>()
}
