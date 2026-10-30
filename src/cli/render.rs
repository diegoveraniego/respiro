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
    let controls_hint = match options.lang {
        Lang::En => "space pause | r reset | q quit",
        Lang::Es => "espacio pausa | r reinicia | q sale",
    };
    let width = width.saturating_sub(2).clamp(1, 61);
    if options.compact || height < 12 || width < 23 {
        let bar_width = width.saturating_sub(2).clamp(1, 31);
        return vec![
            label.into(),
            format!("[{}]", progress(breath.progress, bar_width)),
            if controls {
                controls_hint.into()
            } else {
                "respiro".into()
            },
        ];
    }
    let art_width = width.min(49) | 1;
    let art_height = (height.saturating_sub(7).clamp(5, 17) - 1) | 1;
    let mut canvas = vec![vec![' '; art_width]; art_height];
    let cx = (art_width - 1) as f64 / 2.0;
    let cy = (art_height - 1) as f64 / 2.0;
    let ry = cy - 1.0;
    let rx = (cx - 2.0).min(ry * 2.2);
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
            let (vertices, bow): (Vec<(f64, f64)>, f64) = match active.len() {
                2 => (vec![(0.0, 0.95), (0.0, -0.95)], 0.65),
                3 => (vec![(-0.82, 0.72), (0.0, -0.88), (0.82, 0.72)], 0.24),
                _ => (
                    vec![
                        (-0.76, 0.76),
                        (-0.76, -0.76),
                        (0.76, -0.76),
                        (0.76, 0.76),
                    ],
                    0.24,
                ),
            };
            let n = vertices.len();
            let position = |edge: usize, t: f64| {
                let a = vertices[edge];
                let b = vertices[(edge + 1) % n];
                let dx = b.0 - a.0;
                let dy = b.1 - a.1;
                let len = dx.hypot(dy).max(f64::EPSILON);
                let nx = dy / len;
                let ny = -dx / len;
                let curve = (std::f64::consts::PI * t).sin() * bow;
                let ux = a.0 + dx * t + nx * curve;
                let uy = a.1 + dy * t + ny * curve;
                (cx + ux * rx, cy + uy * ry)
            };
            for edge in 0..n {
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
            for &(vx, vy) in &vertices {
                point(cx + vx * rx, cy + vy * ry, '+');
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
