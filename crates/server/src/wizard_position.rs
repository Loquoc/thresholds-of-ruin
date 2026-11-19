//! Resolve relative wizard coordinates before operations reach the journal.
use crate::journal::Position;

pub(super) fn parse<'a>(
    words: &'a [&'a str],
    origin: Option<Position>,
    usage: &str,
) -> Result<(Position, &'a [&'a str]), String> {
    const OFFSET: &str = "Expected three signed offsets: offset +DX -DY +DZ.";
    match words {
        ["here", rest @ ..] => {
            let mut at = origin.ok_or("here requires an attached actor position.")?;
            if let ["offset", offsets @ ..] = rest {
                let [x, y, z, tail @ ..] = offsets else {
                    return Err(OFFSET.into());
                };
                fn add(at: i32, delta: &str) -> Result<i32, &'static str> {
                    let (sign, digits) = delta.split_at_checked(1).ok_or(OFFSET)?;
                    if !matches!(sign, "+" | "-")
                        || digits.is_empty()
                        || !digits.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(OFFSET);
                    }
                    let range = "Offset is outside the coordinate range.";
                    at.checked_add(delta.parse::<i32>().map_err(|_| range)?)
                        .ok_or(range)
                }
                at.x = add(at.x, x)?;
                at.y = add(at.y, y)?;
                at.z = add(at.z, z)?;
                return Ok((at, tail));
            }
            Ok((at, rest))
        }
        [r, x, y, z, rest @ ..] => Ok((
            Position {
                region: r.parse().map_err(|_| usage)?,
                x: x.parse().map_err(|_| usage)?,
                y: y.parse().map_err(|_| usage)?,
                z: z.parse().map_err(|_| usage)?,
            },
            rest,
        )),
        _ => Err(usage.into()),
    }
}

pub(super) fn exact(
    words: &[&str],
    origin: Option<Position>,
    usage: &str,
) -> Result<Position, String> {
    let (position, rest) = parse(words, origin, usage)?;
    if !rest.is_empty() {
        return Err(usage.into());
    }
    Ok(position)
}
