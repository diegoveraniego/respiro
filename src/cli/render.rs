use super::{Animation, Breath, Lang, Options};

fn progress(t: f64, width: usize) -> String {
    let position = (t * (width - 1) as f64).round() as usize;
    (0..width)
        .map(|x| {
            if x < position {
                '='
            } else if x == position {
                'o'
            } else {
                '.'
            }
        })
        .collect()
}

pub fn frame(
    options: &Options,
    breath: Breath,
    paused: bool,
    width: usize,
    height: usize,
    controls: bool,
) -> Vec<String> {
    let label = match (options.lang, paused) {
        (Lang::En, true) => "Paused",
        (Lang::Es, true) => "Pausa",
        (Lang::En, false) => ["Inhale", "Hold", "Exhale", "Rest"][breath.phase],
        (Lang::Es, false) => ["Inhala", "Manten", "Exhala", "Descansa"][breath.phase],
    };
    let width = width.saturating_sub(2).clamp(1, 61);
    let controls_hint = match (options.lang, width >= 56) {
        (Lang::En, true) => "space pause | r reset | a anim | p pattern | q quit",
        (Lang::En, false) => "space pause | r reset | q quit",
        (Lang::Es, true) => "espacio pausa | r reinicia | a anim | p patron | q sale",
        (Lang::Es, false) => "espacio pausa | r reinicia | q sale",
    };
    if options.compact || height < 12 || width < 23 {
        let bar_width = width.saturating_sub(2).clamp(1, 31);
        return vec![
            label.into(),
            format!("[{}]", progress(breath.progress, bar_width)),
            if controls {
                controls_hint.into()
            } else if options.snapshot {
                "respiro".into()
            } else {
                String::new()
            },
        ];
    }
    let art_width = width.min(49) | 1;
    let art_height = (height.saturating_sub(7).clamp(5, 17) - 1) | 1;
    let mut canvas = vec![vec![' '; art_width]; art_height];
    let cx = (art_width - 1) as f64 / 2.0;
    let cy = (art_height - 1) as f64 / 2.0;
    let rx = cx - 2.0;
    let ry = cy - 1.0;
    let mut point = |x: f64, y: f64, ch: char| {
        let x = x.round().clamp(0.0, (art_width - 1) as f64) as usize;
        let y = y.round().clamp(0.0, (art_height - 1) as f64) as usize;
        canvas[y][x] = ch;
    };
    match options.animation {
        Animation::Orb => {
            let scale = 0.3 + breath.level * 0.7;
            for step in 0..360 {
                let angle = step as f64 * std::f64::consts::TAU / 360.0;
                point(cx + rx * angle.cos(), cy + ry * angle.sin(), '.');
            }
            for step in 0..360 {
                let angle = step as f64 * std::f64::consts::TAU / 360.0;
                point(
                    cx + rx * scale * angle.cos(),
                    cy + ry * scale * angle.sin(),
                    if angle.sin().abs() > 0.7 { '-' } else { ':' },
                );
            }
            point(cx, cy, '+');
        }
        Animation::Path => {
            let active: Vec<_> = (0..4).filter(|&i| options.durations[i] > 0.0).collect();
            let current = active.iter().position(|&i| i == breath.phase).unwrap_or(0);
            let vertices = match active.len() {
                2 => vec![(cx, cy + ry), (cx, cy - ry)],
                3 => vec![(cx - rx, cy + ry), (cx, cy - ry), (cx + rx, cy + ry)],
                _ => vec![
                    (cx - rx, cy + ry),
                    (cx - rx, cy - ry),
                    (cx + rx, cy - ry),
                    (cx + rx, cy + ry),
                ],
            };
            let position = |edge: usize, t: f64| {
                let a = vertices[edge];
                let b = vertices[(edge + 1) % vertices.len()];
                let bow = if vertices.len() == 2 {
                    (std::f64::consts::PI * t).sin() * rx * if edge == 0 { -1.0 } else { 1.0 }
                } else {
                    0.0
                };
                (a.0 + (b.0 - a.0) * t + bow, a.1 + (b.1 - a.1) * t)
            };
            for edge in 0..vertices.len() {
                for step in 0..200 {
                    let t = step as f64 / 199.0;
                    let (x, y) = position(edge, t);
                    point(
                        x,
                        y,
                        if edge == current && t <= breath.progress {
                            '*'
                        } else {
                            '.'
                        },
                    );
                }
            }
            for &(x, y) in &vertices {
                point(x, y, '+');
            }
            let (x, y) = position(current, breath.progress);
            point(x, y, '@');
        }
    }
    let mut lines = vec!["respiro".into(), String::new()];
    lines.extend(canvas.into_iter().map(|row| row.into_iter().collect()));
    lines.push(String::new());
    lines.push(label.into());
    lines.push(format!("[{}]", progress(breath.progress, 21)));
    if controls {
        lines.push(controls_hint.into());
    } else if !options.snapshot {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{breath_at, parse};
    #[test]
    fn every_pattern_renders_ascii_without_numbers_at_all_sizes() {
        for lang in ["en", "es"] {
            for pattern in ["5-5", "4-7-8", "4-4-4-4", "4-0-6-2"] {
                for animation in ["orb", "path"] {
                    let options = parse(
                        ["--lang", lang, "--pattern", pattern, "--animation", animation]
                            .map(String::from),
                    )
                    .unwrap();
                    for (w, h) in [(1, 1), (20, 8), (80, 24)] {
                        for time in [0.0, 4.0, 8.0, 12.0, 16.0] {
                            let lines = frame(
                                &options,
                                breath_at(options.durations, time),
                                false,
                                w,
                                h,
                                true,
                            )
                            .join("\n");
                            assert!(lines.is_ascii());
                            assert!(!lines.chars().any(|c| c.is_ascii_digit() || c == '\x1b'));
                        }
                    }
                }
            }
        }
    }
}
