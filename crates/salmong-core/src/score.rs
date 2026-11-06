const PREFIX: &str = "salmong.best.1 ";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalScore {
    pub score: u64,
    pub ticks: u64,
}

impl FinalScore {
    pub fn encode(self) -> String {
        format!("{PREFIX}{} {}", self.score, self.ticks)
    }

    pub fn decode(text: &str) -> Option<Self> {
        let (score, ticks) = text.strip_prefix(PREFIX)?.split_once(' ')?;
        let run = Self {
            score: score.parse().ok()?,
            ticks: ticks.parse().ok()?,
        };
        (run.encode() == text).then_some(run)
    }
}

pub fn merge(stored: Option<&str>, run: FinalScore) -> FinalScore {
    stored
        .and_then(FinalScore::decode)
        .map_or(run, |old| old.max(run))
}
