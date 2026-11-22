//! `arena-headless` — deterministic scenario runner and combat telemetry tool.
//!
//! With no arguments, runs the original scripted 1D [`Duel`] scenario and
//! prints tick-tagged events. With `--simulate` or any telemetry flag
//! (`--matches`, `--team-size`, `--round-seconds`, `--round-ticks`, `--seed`,
//! `--map`, `--json`, `--per-match`), runs deterministic in-process `Arena`
//! bot matches and emits human-readable or JSON combat telemetry.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arena_headless::runner::{HeadlessConfig, MAX_HEADLESS_MATCHES, run_matches};
use arena_sim::arena::MAX_ROUND_TICKS;
use arena_sim::map::MAX_TEAM;
use arena_sim::{Command, Duel, TICKS_PER_SECOND};

const MAX_ROUND_SECONDS: u64 = MAX_ROUND_TICKS / TICKS_PER_SECOND;

const USAGE: &str = "usage: arena-headless [--simulate] [--matches N] [--team-size 1|2|3] [--round-seconds S | --round-ticks T] [--seed N] [--map PATH] [--per-match] [--json]\n       (with no arguments, runs the scripted 1D duel)";

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    ScriptedDuel,
    Help,
    Simulate {
        config: HeadlessConfig,
        map_path: Option<PathBuf>,
        json: bool,
    },
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Err(message) => {
            eprintln!("arena-headless: {message}");
            ExitCode::FAILURE
        }
        Ok(Mode::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Mode::ScriptedDuel) => {
            run_scripted_duel();
            ExitCode::SUCCESS
        }
        Ok(Mode::Simulate {
            mut config,
            map_path,
            json,
        }) => {
            let map = match load_map(map_path.as_deref()) {
                Ok(map) => map,
                Err(message) => {
                    eprintln!("arena-headless: {message}");
                    return ExitCode::FAILURE;
                }
            };
            config.map_name = map.name.clone();
            match run_matches(&map.layout, &config) {
                Ok(report) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&report).expect("reports serialize")
                        );
                    } else {
                        print!("{report}");
                    }
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("arena-headless: {message}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn load_map(map_path: Option<&Path>) -> Result<arena_map::MapFile, String> {
    match map_path {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            arena_map::parse(&text)
                .map_err(|errors| format!("invalid map {}: {}", path.display(), errors.join("; ")))
        }
        None => Ok(arena_map::colosseum()),
    }
}

fn run_scripted_duel() {
    let mut duel = Duel::default();
    for tick in 0..(5 * TICKS_PER_SECOND) {
        let mut commands = Vec::new();
        if tick == 0 {
            commands.push(Command::Move {
                actor: 0,
                direction: 1,
            });
        }
        if tick % TICKS_PER_SECOND == 0 {
            commands.push(Command::Attack {
                actor: 0,
                target: 1,
            });
        }
        for event in duel.step(&commands) {
            println!("tick={tick}: {event:?}");
        }
        if duel.winner().is_some() {
            break;
        }
    }
    assert_eq!(duel.winner(), Some(0), "scripted duel must finish");
    println!("winner={:?}; state={:?}", duel.winner(), duel.fighters());
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Mode, String> {
    let mut iter = args.into_iter().peekable();
    if iter.peek().is_none() {
        return Ok(Mode::ScriptedDuel);
    }

    let mut config = HeadlessConfig::default();
    let mut map_path = None;
    let mut json = false;
    let mut saw_round_seconds = false;
    let mut saw_round_ticks = false;

    while let Some(arg) = iter.next() {
        let mut value = |name: &str| -> Result<String, String> {
            iter.next()
                .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))
        };
        let number = |name: &str, text: String| -> Result<u64, String> {
            text.parse().map_err(|_| format!("invalid {name} {text}"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(Mode::Help),
            "--simulate" => {}
            "--matches" => {
                let raw = number("--matches", value("--matches")?)?;
                let parsed =
                    usize::try_from(raw).map_err(|_| format!("--matches out of range: {raw}"))?;
                if !(1..=MAX_HEADLESS_MATCHES).contains(&parsed) {
                    return Err(format!(
                        "--matches must be between 1 and {MAX_HEADLESS_MATCHES}, got {parsed}\n{USAGE}"
                    ));
                }
                config.matches = parsed;
            }
            "--team-size" => {
                let raw = number("--team-size", value("--team-size")?)?;
                let parsed =
                    usize::try_from(raw).map_err(|_| format!("--team-size out of range: {raw}"))?;
                if !(1..=MAX_TEAM).contains(&parsed) {
                    return Err(format!(
                        "--team-size must be between 1 and {MAX_TEAM}, got {parsed}\n{USAGE}"
                    ));
                }
                config.team_size = parsed;
            }
            "--round-seconds" => {
                if saw_round_ticks {
                    return Err(format!(
                        "--round-seconds and --round-ticks cannot be used together\n{USAGE}"
                    ));
                }
                let seconds = number("--round-seconds", value("--round-seconds")?)?;
                if !(1..=MAX_ROUND_SECONDS).contains(&seconds) {
                    return Err(format!(
                        "--round-seconds must be between 1 and {MAX_ROUND_SECONDS}, got {seconds}\n{USAGE}"
                    ));
                }
                config.round_ticks = seconds * TICKS_PER_SECOND;
                saw_round_seconds = true;
            }
            "--round-ticks" => {
                if saw_round_seconds {
                    return Err(format!(
                        "--round-seconds and --round-ticks cannot be used together\n{USAGE}"
                    ));
                }
                let ticks = number("--round-ticks", value("--round-ticks")?)?;
                if !(1..=MAX_ROUND_TICKS).contains(&ticks) {
                    return Err(format!(
                        "--round-ticks must be between 1 and {MAX_ROUND_TICKS}, got {ticks}\n{USAGE}"
                    ));
                }
                config.round_ticks = ticks;
                saw_round_ticks = true;
            }
            "--seed" => {
                config.seed = number("--seed", value("--seed")?)?;
            }
            "--map" => {
                map_path = Some(PathBuf::from(value("--map")?));
            }
            "--per-match" => {
                config.include_per_match = true;
            }
            "--json" => {
                json = true;
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }

    config.validate()?;
    Ok(Mode::Simulate {
        config,
        map_path,
        json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn no_args_runs_scripted_duel_for_backwards_compatibility() {
        assert_eq!(parse_args(Vec::new()), Ok(Mode::ScriptedDuel));
        assert_eq!(parse_args(args(&["--help"])), Ok(Mode::Help));
    }

    #[test]
    fn simulation_flags_parse_and_validate_bounds() {
        let mode = parse_args(args(&[
            "--matches",
            "5",
            "--team-size",
            "3",
            "--round-seconds",
            "60",
            "--seed",
            "99",
            "--per-match",
            "--json",
        ]))
        .unwrap();
        match mode {
            Mode::Simulate {
                config,
                map_path,
                json,
            } => {
                assert_eq!(config.matches, 5);
                assert_eq!(config.team_size, 3);
                assert_eq!(config.round_ticks, 60 * TICKS_PER_SECOND);
                assert_eq!(config.seed, 99);
                assert!(config.include_per_match);
                assert!(map_path.is_none());
                assert!(json);
            }
            other => panic!("expected Simulate, got {other:?}"),
        }

        // Invalid bounds and conflicting options are rejected cleanly.
        for bad in [
            &["--matches", "0"][..],
            &["--matches", "10001"][..],
            &["--matches", "abc"][..],
            &["--team-size", "0"][..],
            &["--team-size", "4"][..],
            &["--round-seconds", "0"][..],
            &["--round-seconds", "1201"][..],
            &["--round-ticks", "0"][..],
            &["--round-ticks", "36001"][..],
            &["--round-seconds", "10", "--round-ticks", "300"][..],
            &["--round-ticks", "300", "--round-seconds", "10"][..],
            &["--map"][..],
            &["--unknown"][..],
            &["--seed"][..],
        ] {
            assert!(parse_args(args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn load_map_handles_default_valid_missing_and_invalid_files() {
        let default_map = load_map(None).unwrap();
        assert_eq!(default_map.name, "Colosseum");

        let colosseum_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps/colosseum.json");
        let loaded = load_map(Some(&colosseum_path)).unwrap();
        assert_eq!(loaded, default_map);

        let missing = load_map(Some(Path::new("/nonexistent/arena-map-missing.json")));
        assert!(missing.is_err());

        // Passing a non-map file (Cargo.toml) fails JSON/map validation cleanly.
        let invalid_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let invalid = load_map(Some(&invalid_path)).unwrap_err();
        assert!(invalid.contains("invalid map"), "{invalid}");
    }
}
