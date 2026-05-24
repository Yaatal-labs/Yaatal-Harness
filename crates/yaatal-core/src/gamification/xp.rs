use crate::design::tokens;

#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub number: u32,
    pub title: &'static str,
    pub emoji: &'static str,
    pub color: &'static str,
    pub min_xp: u32,
    pub max_xp: u32,
}

pub const LEVELS: &[Level] = &[
    Level {
        number: 1,
        title: "Learner",
        emoji: "\u{1F331}",
        color: tokens::FOREST_GREEN,
        min_xp: 0,
        max_xp: 499,
    },
    Level {
        number: 2,
        title: "Builder",
        emoji: "\u{1F528}",
        color: tokens::TERRACOTTA_PRIMARY,
        min_xp: 500,
        max_xp: 1499,
    },
    Level {
        number: 3,
        title: "Innovator",
        emoji: "\u{1F4A1}",
        color: tokens::SAVANNA_GOLD,
        min_xp: 1500,
        max_xp: 3499,
    },
    Level {
        number: 4,
        title: "Architect",
        emoji: "\u{1F3DB}\u{FE0F}",
        color: tokens::INDIGO_DEEP,
        min_xp: 3500,
        max_xp: 6999,
    },
    Level {
        number: 5,
        title: "Elder",
        emoji: "\u{1F451}",
        color: tokens::SAVANNA_GOLD,
        min_xp: 7000,
        max_xp: 14999,
    },
    Level {
        number: 6,
        title: "Griot",
        emoji: "\u{1F4D6}",
        color: tokens::TERRACOTTA_PRIMARY,
        min_xp: 15000,
        max_xp: u32::MAX,
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum XpAction {
    ReadArticle,
    Upvote,
    Comment,
    PostArticle,
    HelpfulComment,
    TutorialCompletion,
    AnswerQuestion,
    SolutionAccepted,
    DailyStreak,
    WeeklyChallenge,
}

impl XpAction {
    pub fn points(&self) -> u32 {
        match self {
            XpAction::ReadArticle => 5,
            XpAction::Upvote => 2,
            XpAction::Comment => 10,
            XpAction::PostArticle => 25,
            XpAction::HelpfulComment => 50,
            XpAction::TutorialCompletion => 100,
            XpAction::AnswerQuestion => 30,
            XpAction::SolutionAccepted => 150,
            XpAction::DailyStreak => 20,
            XpAction::WeeklyChallenge => 200,
        }
    }
}

pub fn level_for_xp(xp: u32) -> &'static Level {
    LEVELS
        .iter()
        .rev()
        .find(|l| xp >= l.min_xp)
        .unwrap_or(&LEVELS[0])
}

pub fn level_progress(xp: u32) -> f32 {
    let level = level_for_xp(xp);
    if level.max_xp == u32::MAX {
        return 1.0;
    }
    let range = level.max_xp - level.min_xp;
    let progress = xp - level.min_xp;
    (progress as f32) / (range as f32)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_level_for_zero_xp() {
        let level = level_for_xp(0);
        assert_eq!(level.title, "Learner");
        assert_eq!(level.number, 1);
    }

    #[test]
    fn test_level_for_builder() {
        assert_eq!(level_for_xp(500).title, "Builder");
    }

    #[test]
    fn test_level_for_griot() {
        assert_eq!(level_for_xp(15000).title, "Griot");
    }

    #[test]
    fn test_level_progress_start() {
        assert_eq!(level_progress(0), 0.0);
    }

    #[test]
    fn test_level_progress_mid() {
        let p = level_progress(250);
        assert!(p > 0.4 && p < 0.6);
    }

    #[test]
    fn test_xp_action_points() {
        assert_eq!(XpAction::SolutionAccepted.points(), 150);
        assert_eq!(XpAction::ReadArticle.points(), 5);
    }

    #[test]
    fn test_levels_are_contiguous() {
        for i in 0..LEVELS.len() - 1 {
            assert_eq!(
                LEVELS[i].max_xp + 1,
                LEVELS[i + 1].min_xp,
                "Gap between {} and {}",
                LEVELS[i].title,
                LEVELS[i + 1].title
            );
        }
    }
}
