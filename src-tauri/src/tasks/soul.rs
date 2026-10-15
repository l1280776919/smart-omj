use crate::vision::Match;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Preparing,
    Entering,
    Battle,
    Victory,
    Reward,
    Returning,
}
impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preparing => "检查御魂界面",
            Self::Entering => "进入战斗",
            Self::Battle => "等待战斗",
            Self::Victory => "胜利结算",
            Self::Reward => "领取奖励",
            Self::Returning => "验证返回",
        }
    }
}
#[derive(Debug, PartialEq)]
pub enum Action {
    Wait,
    Tap(u32, u32),
    TapReference(u32, u32),
    RoundComplete,
}
pub struct SoulMachine {
    pub stage: Stage,
    pub layer: u32,
    actions_in_stage: u32,
}
impl SoulMachine {
    pub fn new(layer: u32) -> Self {
        Self {
            stage: Stage::Preparing,
            layer,
            actions_in_stage: 0,
        }
    }
    fn transition(&mut self, stage: Stage) {
        self.stage = stage;
        self.actions_in_stage = 0;
    }
    fn tap(&mut self, x: u32, y: u32) -> Result<Action, String> {
        self.actions_in_stage += 1;
        if self.actions_in_stage > 3 {
            return Err("同阶段最多操作 3 次仍未得到确认，已停止".into());
        }
        Ok(Action::Tap(x, y))
    }
    fn tap_reference(&mut self, x: u32, y: u32) -> Result<Action, String> {
        self.tap(x, y)?;
        Ok(Action::TapReference(x, y))
    }
    pub fn step(&mut self, matches: &[Match]) -> Result<Action, String> {
        let found = |id: &str| matches.iter().find(|m| m.id == id && m.found);
        if found("defeat").is_some() {
            return Err("检测到战斗失败，已停止".into());
        }
        match self.stage {
            Stage::Preparing => {
                if found(&format!("layer_{}", self.layer)).is_none() || found("locked").is_none() {
                    return Err("请先进入指定御魂层挑战页，并锁定已预设阵容".into());
                }
                let target = found("challenge").ok_or("未识别到挑战按钮，请检查界面或体力提示")?;
                let [x, y, w, h] = target.bounds;
                self.transition(Stage::Entering);
                self.tap(x + w / 2, y + h / 2)
            }
            Stage::Entering => {
                if found("battle").is_some() {
                    self.transition(Stage::Battle);
                    return Ok(Action::Wait);
                }
                if let Some(target) = found("ready") {
                    let [x, y, w, h] = target.bounds;
                    return self.tap(x + w / 2, y + h / 2);
                }
                Ok(Action::Wait) // Never blindly repeat the challenge that consumes stamina.
            }
            Stage::Battle => {
                if found("victory").is_some() {
                    self.transition(Stage::Victory);
                    return self.tap_reference(1175, 350);
                }
                Ok(Action::Wait)
            }
            Stage::Victory => {
                if found("reward").is_some() {
                    self.transition(Stage::Reward);
                    return self.tap_reference(1175, 350);
                }
                Ok(Action::Wait)
            }
            Stage::Reward => {
                if found("challenge").is_some() && found(&format!("layer_{}", self.layer)).is_some()
                {
                    self.transition(Stage::Returning);
                    return Ok(Action::RoundComplete);
                }
                if found("reward").is_some() {
                    return self.tap_reference(1175, 350);
                }
                Ok(Action::Wait)
            }
            Stage::Returning => {
                self.transition(Stage::Preparing);
                self.step(matches)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observations(ids: &[&str]) -> Vec<Match> {
        ids.iter()
            .map(|id| Match {
                id: (*id).into(),
                found: true,
                score: 1.0,
                bounds: [10, 20, 20, 10],
            })
            .collect()
    }
    #[test]
    fn full_round_requires_battle_victory_reward_and_return() {
        let mut m = SoulMachine::new(11);
        assert_eq!(
            m.step(&observations(&["layer_11", "locked", "challenge"]))
                .unwrap(),
            Action::Tap(20, 25)
        );
        assert_eq!(m.step(&observations(&["challenge"])).unwrap(), Action::Wait);
        m.step(&observations(&["battle"])).unwrap();
        assert_eq!(m.step(&observations(&["challenge"])).unwrap(), Action::Wait);
        m.step(&observations(&["victory"])).unwrap();
        m.step(&observations(&["reward"])).unwrap();
        assert_eq!(
            m.step(&observations(&["challenge", "layer_11"])).unwrap(),
            Action::RoundComplete
        );
    }
    #[test]
    fn wrong_layer_unlocked_and_defeat_stop() {
        for ids in [
            vec!["layer_10", "locked", "challenge"],
            vec!["layer_11", "challenge"],
            vec!["defeat"],
        ] {
            assert!(SoulMachine::new(11).step(&observations(&ids)).is_err());
        }
    }
    #[test]
    fn ready_click_is_bounded() {
        let mut m = SoulMachine::new(11);
        m.stage = Stage::Entering;
        for _ in 0..3 {
            assert!(m.step(&observations(&["ready"])).is_ok());
        }
        assert!(m.step(&observations(&["ready"])).is_err());
    }
}
