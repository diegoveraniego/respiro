#[path = "../patterns.rs"]
mod patterns;
mod render;
mod terminal;

use std::{
    env,
    io::{self, IsTerminal, Write},
    process::ExitCode,
    time::{Duration, Instant},
};

const HELP_EN: &str = "Respiro · Meditation from your terminal — because touching grass isn't POSIX-compliant.

Usage: respiro-cli [options]

  -p, --pattern PATTERN  5-5-5-5 (default), 4-4-4-4, 4-7-8, 5-5
                         Aliases: box, relax, coherent. Or custom durations.
                         Two values: inhale-exhale. Three: inhale-hold-exhale.
                         Four: inhale-hold-exhale-rest.
                         Inhale/exhale: 1..30 s; holds: 0..30 s; decimals allowed.
  -a, --animation TYPE   orb (expanding circle) or path (phase perimeter)
  -c, --cycles N         Stop after N full cycles; unlimited by default
  -l, --lang LANG        en (default) or es (Spanish translation)
      --compact          Three-line inline animation inside current terminal
      --snapshot         Print a static ASCII frame without ANSI controls and exit
      --list-patterns    List available breathing patterns
  -h, --help             Show this help
  -V, --version          Show version

Controls: Space pause/resume · r reset · q / Esc / Ctrl+C quit
No timers or numerical countdowns are shown during the session. Requires an ANSI terminal.
Examples:
  respiro-cli --pattern 4-7-8 --animation path
  respiro-cli --pattern 4-0-6-0 --compact --cycles 3
  respiro-cli --lang es --pattern relax
  fastfetch --logo-type command-raw --logo 'respiro-cli --snapshot'
";

const HELP_ES: &str = "Respiro · Meditacion desde tu terminal — porque tocar pasto no es compatible con POSIX.

Uso: respiro-cli [opciones]

  -p, --pattern PATRON   5-5-5-5 (por defecto), 4-4-4-4, 4-7-8, 5-5
                         Alias: box, relax, coherent. O tiempos personalizados.
                         Dos valores: inhala-exhala. Tres: inhala-manten-exhala.
                         Cuatro: inhala-manten-exhala-descansa.
                         Inhala/exhala: 1..30 s; pausas: 0..30 s; admite decimales.
  -a, --animation TIPO   orb (expansion) o path (recorrido por fase)
  -c, --cycles N         Termina tras N ciclos completos; por defecto sin limite
  -l, --lang IDIOMA      en (por defecto) o es (traduccion al espanol)
      --compact          Animacion de tres lineas dentro de la terminal
      --snapshot         Imprime un fotograma ASCII sin controles ANSI y termina
      --list-patterns    Muestra los patrones disponibles
  -h, --help             Muestra esta ayuda
  -V, --version          Muestra la version

Controles: Espacio pausa/reanuda · r reinicia · q / Esc / Ctrl+C sale
La sesion no muestra tiempos ni cuentas numericas. Requiere una terminal ANSI.
Ejemplos:
  respiro-cli --pattern 4-7-8 --animation path
  respiro-cli --pattern 4-0-6-0 --compact --cycles 3
  respiro-cli --lang es --pattern relax
  fastfetch --logo-type command-raw --logo 'respiro-cli --snapshot'
";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Lang {
    En,
    Es,
}

impl Lang {
    fn from_str(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "en" | "eng" | "english" => Ok(Self::En),
            "es" | "esp" | "spanish" | "espanol" | "español" => Ok(Self::Es),
            _ => Err(format!("unsupported language '{value}': use 'en' or 'es'")),
        }
    }

    fn from_env() -> Self {
        env::var("RESPIRO_LANG")
            .ok()
            .and_then(|v| Self::from_str(&v).ok())
            .unwrap_or(Self::En)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Animation {
    Orb,
    Path,
}

#[derive(Debug)]
struct Options {
    durations: [f64; 4],
    animation: Animation,
    cycles: Option<u32>,
    compact: bool,
    snapshot: bool,
    lang: Lang,
}

fn pattern(value: &str, lang: Lang) -> Result<[f64; 4], String> {
    let value = match value {
        "box" => "4-4-4-4",
        "relax" => "4-7-8",
        "coherent" => "5-5",
        other => other,
    };
    if let Some(p) = patterns::PRESETS
        .iter()
        .find(|p| p.name == value && p.name != "Personalizado")
    {
        return Ok(p.durations);
    }
    let parts: Result<Vec<f64>, _> = value.split(['-', ',']).map(str::parse).collect();
    let d = match parts.as_deref() {
        Ok([i, e]) => [*i, 0.0, *e, 0.0],
        Ok([i, h, e]) => [*i, *h, *e, 0.0],
        Ok([i, h, e, r]) => [*i, *h, *e, *r],
        _ => {
            return Err(match lang {
                Lang::En => "invalid pattern: use 2, 3, or 4 durations separated by '-' or ','".into(),
                Lang::Es => "patron invalido: usa 2, 3 o 4 tiempos separados por '-' o ','".into(),
            });
        }
    };
    if d.iter()
        .enumerate()
        .any(|(i, v)| !v.is_finite() || *v < if i % 2 == 0 { 1.0 } else { 0.0 } || *v > 30.0)
    {
        return Err(match lang {
            Lang::En => "inhale/exhale must be 1..30 s and holds 0..30 s".into(),
            Lang::Es => "inhala/exhala deben durar 1..30 s y las pausas 0..30 s".into(),
        });
    }
    Ok(d)
}

fn detect_lang_arg(args: &[String]) -> Lang {
    let mut iter = args.iter();
    let mut lang = Lang::from_env();
    while let Some(arg) = iter.next() {
        if arg == "-l" || arg == "--lang" {
            if let Some(val) = iter.next() {
                if let Ok(parsed) = Lang::from_str(val) {
                    lang = parsed;
                }
            }
        } else if let Some(val) = arg.strip_prefix("--lang=") {
            if let Ok(parsed) = Lang::from_str(val) {
                lang = parsed;
            }
        }
    }
    lang
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let collected: Vec<String> = args.into_iter().collect();
    let mut lang = detect_lang_arg(&collected);
    let mut options = Options {
        durations: patterns::PRESETS[0].durations,
        animation: Animation::Orb,
        cycles: None,
        compact: false,
        snapshot: false,
        lang,
    };
    let mut raw_pattern: Option<String> = None;
    let mut iter = collected.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--compact" => options.compact = true,
            "--snapshot" => options.snapshot = true,
            "-l" | "--lang" => {
                let val = iter.next().ok_or(match lang {
                    Lang::En => "missing language code (use 'en' or 'es')",
                    Lang::Es => "falta el idioma (usa 'en' o 'es')",
                })?;
                lang = Lang::from_str(&val)?;
                options.lang = lang;
            }
            _ if arg.starts_with("--lang=") => {
                let val = &arg["--lang=".len()..];
                lang = Lang::from_str(val)?;
                options.lang = lang;
            }
            "-p" | "--pattern" => {
                raw_pattern = Some(iter.next().ok_or(match lang {
                    Lang::En => "missing pattern",
                    Lang::Es => "falta el patron",
                })?);
            }
            "-a" | "--animation" => {
                options.animation = match iter.next().as_deref() {
                    Some("orb") => Animation::Orb,
                    Some("path") => Animation::Path,
                    _ => {
                        return Err(match lang {
                            Lang::En => "invalid animation: use 'orb' or 'path'".into(),
                            Lang::Es => "animacion invalida: usa orb o path".into(),
                        });
                    }
                }
            }
            "-c" | "--cycles" => {
                let n = iter
                    .next()
                    .ok_or(match lang {
                        Lang::En => "missing cycle count",
                        Lang::Es => "falta el numero de ciclos",
                    })?
                    .parse::<u32>()
                    .map_err(|_| match lang {
                        Lang::En => "cycles must be a positive integer",
                        Lang::Es => "ciclos debe ser un entero positivo",
                    })?;
                if n == 0 {
                    return Err(match lang {
                        Lang::En => "cycles must be greater than zero".into(),
                        Lang::Es => "ciclos debe ser mayor que cero".into(),
                    });
                }
                options.cycles = Some(n);
            }
            _ => {
                return Err(match lang {
                    Lang::En => format!("unknown option: {arg}; use --help"),
                    Lang::Es => format!("opcion desconocida: {arg}; usa --help"),
                });
            }
        }
    }
    if let Some(pat) = raw_pattern {
        options.durations = pattern(&pat, options.lang)?;
    }
    Ok(options)
}

#[derive(Debug, Clone, Copy)]
struct Breath {
    phase: usize,
    progress: f64,
    level: f64,
}

fn breath_at(durations: [f64; 4], elapsed: f64) -> Breath {
    let mut time = elapsed.rem_euclid(durations.iter().sum());
    for (phase, duration) in durations.into_iter().enumerate() {
        if time < duration {
            let progress = time / duration;
            let smooth = progress * progress * (3.0 - 2.0 * progress);
            let level = match phase {
                0 => smooth,
                1 => 1.0,
                2 => 1.0 - smooth,
                _ => 0.0,
            };
            return Breath {
                phase,
                progress,
                level,
            };
        }
        time -= duration;
    }
    Breath {
        phase: 0,
        progress: 0.0,
        level: 0.0,
    }
}

fn run(options: Options) -> io::Result<()> {
    if options.snapshot {
        let breath = breath_at(options.durations, options.durations[0] * 0.65);
        let lines = render::frame(&options, breath, false, 39, 19, false);
        let width = lines.iter().map(String::len).max().unwrap_or(0);
        let lines: Vec<_> = lines
            .iter()
            .map(|line| format!("{line:^width$}").trim_end().to_string())
            .collect();
        return writeln!(io::stdout().lock(), "{}", lines.join("\n"));
    }
    if !io::stdin().is_terminal()
        || !io::stdout().is_terminal()
        || env::var("TERM").is_ok_and(|v| v == "dumb")
    {
        return Err(io::Error::other(match options.lang {
            Lang::En => "use an interactive ANSI terminal; for Fastfetch or piping use --snapshot",
            Lang::Es => "usa una terminal ANSI interactiva; para Fastfetch o redireccionar usa --snapshot",
        }));
    }
    let mut terminal = terminal::Terminal::enter(options.compact)?;
    let mut elapsed = Duration::ZERO;
    let mut last = Instant::now();
    let mut paused = false;
    let limit = options
        .cycles
        .map(|n| options.durations.iter().sum::<f64>() * n as f64);
    loop {
        let now = Instant::now();
        if !paused {
            elapsed += now.duration_since(last);
        }
        last = now;
        if terminal::interrupted() || limit.is_some_and(|limit| elapsed.as_secs_f64() >= limit) {
            break;
        }
        let (width, height) = terminal.size();
        let lines = render::frame(
            &options,
            breath_at(options.durations, elapsed.as_secs_f64()),
            paused,
            width,
            height,
            true,
        );
        terminal.draw(&lines, width, height)?;
        if let Some(key) = terminal.key(Duration::from_millis(40))? {
            // Account for time waiting for input before changing the pause state.
            let now = Instant::now();
            if !paused {
                elapsed += now.duration_since(last);
            }
            last = now;
            match key {
                b'q' | b'Q' | 27 | 3 | 4 => break,
                b' ' => paused = !paused,
                b'r' | b'R' => elapsed = Duration::ZERO,
                _ => {}
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let lang = detect_lang_arg(&args);
    if args.iter().any(|s| s == "--help" || s == "-h") {
        print!("{}", match lang {
            Lang::En => HELP_EN,
            Lang::Es => HELP_ES,
        });
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|s| s == "--version" || s == "-V") {
        println!("respiro-cli {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|s| s == "--list-patterns") {
        match lang {
            Lang::En => println!(
                "5-5-5-5    Gentle box breathing (default)\n4-4-4-4    Box breathing · box\n4-7-8      Deep relaxation hold · relax\n5-5        Coherent breathing · coherent\nI-H-E-R    Custom durations; zero skips a hold phase"
            ),
            Lang::Es => println!(
                "5-5-5-5    Cuadrada suave (por defecto)\n4-4-4-4    Cuadrada · box\n4-7-8      Con pausa al inhalar · relax\n5-5        Coherente · coherent\nI-H-E-R    Personalizado; cero omite una pausa"
            ),
        }
        return ExitCode::SUCCESS;
    }
    match parse(args).and_then(|options| run(options).map_err(|e| e.to_string())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("respiro-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns_validate_and_map_phases() {
        assert_eq!(pattern("coherent", Lang::En).unwrap(), [5.0, 0.0, 5.0, 0.0]);
        assert_eq!(pattern("4-7-8", Lang::En).unwrap(), [4.0, 7.0, 8.0, 0.0]);
        assert_eq!(pattern("2.5,0,6,1", Lang::Es).unwrap(), [2.5, 0.0, 6.0, 1.0]);
        for invalid in ["0-0", "NaN-5", "inf-5", "31-5", "4--2-6", "1", "1-2-3-4-5"] {
            assert!(pattern(invalid, Lang::En).is_err(), "{invalid}");
            assert!(pattern(invalid, Lang::Es).is_err(), "{invalid}");
        }
    }
    #[test]
    fn phase_boundaries_skip_zero_holds_and_wrap() {
        let d = pattern("5-5", Lang::En).unwrap();
        assert_eq!(breath_at(d, 5.0).phase, 2);
        assert_eq!(breath_at(d, 5.0).level, 1.0);
        assert_eq!(breath_at(d, 10.0).phase, 0);
        assert_eq!(breath_at(d, 102.5).level, 0.5);
        let d = pattern("4-7-8", Lang::En).unwrap();
        assert_eq!(breath_at(d, 4.0).phase, 1);
        assert_eq!(breath_at(d, 11.0).phase, 2);
        assert_eq!(breath_at(d, 19.0).phase, 0);
    }
    #[test]
    fn rejects_invalid_options_and_supports_lang() {
        assert_eq!(
            parse(["--lang", "es"].map(String::from)).unwrap().lang,
            Lang::Es
        );
        assert_eq!(parse(["--compact"].map(String::from)).unwrap().lang, Lang::En);
        for args in [
            vec!["--cycles", "0"],
            vec!["--cycles", "-1"],
            vec!["--pattern"],
            vec!["--animation", "bad"],
            vec!["--lang", "fr"],
            vec!["--lang"],
        ] {
            assert!(parse(args.into_iter().map(String::from)).is_err());
        }
    }
}
