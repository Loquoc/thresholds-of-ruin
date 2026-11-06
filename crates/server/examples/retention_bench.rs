//! One isolated retention workload per process; no public gameplay surface.
fn arguments(args: &[String]) -> Result<(usize, usize), &'static str> {
    if args.len() != 2 {
        return Err("usage: retention_bench HISTORY OWNERS");
    }
    let history = args[0].parse().map_err(|_| "integer history required")?;
    let owners = args[1].parse().map_err(|_| "integer owners required")?;
    if ![0, 100, 1000, 10000].contains(&history) || ![1, 8].contains(&owners) {
        return Err("history must be 0/100/1000/10000; owners must be 1/8");
    }
    Ok((history, owners))
}
fn main() {
    let (history, owners) = arguments(&std::env::args().skip(1).collect::<Vec<_>>())
        .unwrap_or_else(|message| {
            eprintln!("{message}");
            std::process::exit(2);
        });
    let directory = tempfile::tempdir().expect("temporary save directory");
    let sample =
        tor_server::run_retention_profile(history, owners, &directory.path().join("retention.db"));
    println!("retention-sample {sample}");
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(values: &[&str]) -> Result<(usize, usize), &'static str> {
        arguments(
            &values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn accepts_only_bounded_matrix_arguments() {
        for history in ["0", "100", "1000", "10000"] {
            for owners in ["1", "8"] {
                assert_eq!(
                    parse(&[history, owners]),
                    Ok((history.parse().unwrap(), owners.parse().unwrap()))
                );
            }
        }
        for values in [
            vec![],
            vec!["0"],
            vec!["0", "1", "extra"],
            vec!["-1", "1"],
            vec!["x", "1"],
            vec!["1", "1"],
            vec!["0", "0"],
            vec!["0", "9"],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
    }
}
