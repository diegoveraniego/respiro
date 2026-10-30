#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    pub name: &'static str,
    pub durations: [f64; 4], // [inhale, hold1, exhale, hold2] in seconds
}

pub const PRESETS: &[Pattern] = &[
    Pattern {
        name: "5-5-5-5",
        durations: [5.0, 5.0, 5.0, 5.0],
    },
    Pattern {
        name: "4-4-4-4",
        durations: [4.0, 4.0, 4.0, 4.0],
    },
    Pattern {
        name: "4-7-8",
        durations: [4.0, 7.0, 8.0, 0.0],
    },
    Pattern {
        name: "5-5",
        durations: [5.0, 0.0, 5.0, 0.0],
    },
    Pattern {
        name: "Personalizado",
        durations: [5.0, 5.0, 5.0, 5.0],
    },
];
