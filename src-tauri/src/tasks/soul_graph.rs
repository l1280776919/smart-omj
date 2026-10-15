//! Page evidence and verified transitions; independent of inference and ADB.
use crate::vision::detector::{Detection, Observation, iou};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Page { Courtyard, Explore, SoulMenu, LayerSelect, Challenge, Prepare, Battle, Victory, Reward, Loading, Defeat, StaminaEmpty, NetworkError, Unknown }
impl Page {
    pub fn label(self)-> &'static str {
        match self {
            Self::Courtyard=>"庭院", Self::Explore=>"探索页", Self::SoulMenu=>"御魂入口",
            Self::LayerSelect=>"选择目标层数",Self::Challenge=>"目标层挑战页",Self::Prepare=>"战斗准备",
            Self::Battle=>"战斗中",Self::Victory=>"胜利",Self::Reward=>"奖励结算",
            Self::Loading=>"加载中",Self::Defeat=>"战斗失败",Self::StaminaEmpty=>"体力不足",
            Self::NetworkError=>"网络提示",Self::Unknown=>"未知页面",
        }
    }
}
// Confidence and plausible position are both required. Ambiguous duplicate
// instances are rejected instead of choosing one by chance.
fn min_confidence(class: &str) -> f32 {
    match class {
        // Evidence only; never a click target. A reviewed explore frame localized this
        // marker correctly at 0.716, just under the action threshold.
        "explore_marker" => 0.70,
        _ => 0.75,
    }
}
pub fn element<'a>(o:&'a Observation, class:&str)->Option<&'a Detection> {
    let min=min_confidence(class);
    let mut matches=o.detections.iter().filter(|d|d.class==class && d.confidence>=min && d.confidence.is_finite());
    let first=matches.next()?;
    if matches.next().is_some() { return None; }
    let [x,y]=first.center();
    let [w,h]=o.dimensions;
    if x>=w || y>=h || w==0 || h==0 { return None; }
    let (x,y)=(x as f32/w as f32,y as f32/h as f32);
    let plausible=match class {
        "explore"=> x>0.3&&x<0.7&&y<0.5,
        "summon"=>x>0.65&&y<0.55,
        "soulbtn"=>x<0.35&&y>0.8,
        "soul_entry"=>x>0.25&&x<0.65&&y>0.2&&y<0.75,
        "explore_marker"=>x>0.65&&y<0.5,
        "soul_title"=>x<0.3&&y<0.2,
        "orochi_entry"=>x<0.4&&y>0.1&&y<0.7,
        "orochi_title"=>x>0.6&&y<0.4,
        "layer1_option"|"layer1_selected"=>x<0.3&&y<0.3,
        "challenge"|"ready"=>x>0.75&&y>0.65,
        "prepare_marker"|"continue"=>x>0.3&&x<0.7&&y>0.85,
        "auto_battle"=>x<0.15&&y>0.75,
        "battle_controls"=>x<0.35&&y<0.2,
        "reward"=>x>0.3&&x<0.7&&y>0.45,
        _=>true,
    };
    plausible.then_some(first)
}
pub fn classify(o:&Observation)->Page {
    let has=|id|element(o,id).is_some();
    for (id,page) in [("network_error",Page::NetworkError),("stamina_empty",Page::StaminaEmpty),("defeat",Page::Defeat)] {
        if has(id) {return page;}
    }
    let mut candidates=Vec::new();
    if has("explore")&&has("summon") {candidates.push(Page::Courtyard);}
    // The map pin moves, and the fixed corner marker score drops on live frames
    // (reviewed boxes at 0.66–0.72). The back control plus one soul pin is stable
    // and does not appear together on the other labeled pages.
    if has("soul_entry") && (has("explore_marker") || has("back")) {candidates.push(Page::Explore);}
    if has("soul_title")&&has("orochi_entry") {candidates.push(Page::SoulMenu);}
    if has("soul_title")&&has("orochi_title")&&has("challenge") {
        if has("layer1_selected")&&!has("other_layer_selected")&&!has("layer1_option") { candidates.push(Page::Challenge); }
        else if has("layer1_option")&&!has("layer1_selected") {candidates.push(Page::LayerSelect);}
    }
    if has("ready")&&has("prepare_marker") {candidates.push(Page::Prepare);}
    if has("auto_battle")&&has("battle_controls") {candidates.push(Page::Battle);}
    if has("victory") {candidates.push(Page::Victory);}
    if has("reward")&&has("continue") {candidates.push(Page::Reward);}
    if has("loading") {candidates.push(Page::Loading);}
    if candidates.len()==1 {candidates[0]} else {Page::Unknown}
}
pub fn classify_soul11(o:&Observation)->Page {
    let has=|id|element(o,id).is_some();
    if has("scene_fail") || has("defeat") {return Page::Defeat;}
    if has("network_error") {return Page::NetworkError;}
    if has("stamina_empty") {return Page::StaminaEmpty;}
    let mut pages=Vec::new();
    if has("scene_yard") && has("explore") {pages.push(Page::Courtyard);}
    if has("scene_explore") && has("soulbtn") {pages.push(Page::Explore);}
    if has("scene_mitama") && has("orochi_entry") {pages.push(Page::SoulMenu);}
    if has("scene_orochi") && has("layer11_selected") && has("formation_locked") && !has("formation_unlocked") && has("challenge")
        && !["layer10_selected","layer12_selected","layer13_selected"].iter().any(|id|has(id)) {pages.push(Page::Challenge);}
    if has("scene_ready") && has("ready") {pages.push(Page::Prepare);}
    if has("scene_fight") {pages.push(Page::Battle);}
    if has("scene_results") && has("continue") {pages.push(Page::Reward);}
    if pages.len()==1 {pages[0]}else{Page::Unknown}
}
#[derive(Debug,Clone,Serialize)]
pub struct Plan {
    pub from: Page,
    pub target: String,
    pub bounds: [f32;4],
    pub point: [u32;2],
    pub expected: Vec<Page>,
    pub timeout_ms: u64,
    pub retry_limit: u32,
}
#[derive(Debug,Clone,Serialize)]
pub enum Decision { Wait, Tap(Plan), Complete }
#[derive(Debug)]
struct Pending { plan:Plan, sent_at:u64, last_attempt:u64, attempts:u32 }
pub struct Controller {
    pub page:Page,
    pub completed:u32,
    goal:u32,
    layer:u32,
    unverified_layer_frames:u32,
    stable:u32,
    page_since:u64,
    pending:Option<Pending>,
    owned_round:bool,
    battle_seen:bool,
    reward_seen:bool,
    timeout_ms:u64,
}
impl Controller {
    pub fn new(goal:u32,timeout_ms:u64)->Self {
        Self {page:Page::Unknown,completed:0,goal,layer:1,unverified_layer_frames:0,stable:0,page_since:0,pending:None,owned_round:false,battle_seen:false,reward_seen:false,timeout_ms}
    }
    pub fn soul11(goal:u32,timeout_ms:u64)->Self {let mut controller=Self::new(goal,timeout_ms);controller.layer=11;controller}
    pub fn tick(&mut self,o:&Observation,now:u64)->Result<Decision,String> {
        let page=if self.layer==11 {classify_soul11(o)}else{classify(o)};
        if self.layer==11 && element(o,"scene_orochi").is_some() && page!=Page::Challenge {
            self.unverified_layer_frames+=1;
            self.page=Page::Unknown;
            if self.unverified_layer_frames>=2 {
                let mut missing=Vec::new();
                if element(o,"layer11_selected").is_none(){missing.push("未识别到巫女大蛇标题，请选择悲鸣");}
                if element(o,"formation_unlocked").is_some(){missing.push("阵容当前未锁定（开锁图标），请锁定预设阵容");}
                else if element(o,"formation_locked").is_none(){missing.push("未识别到闭锁图标，无法确认阵容锁定状态");}
                if element(o,"challenge").is_none(){missing.push("未识别到挑战按钮");}
                if ["layer10_selected","layer12_selected","layer13_selected"].iter().any(|id|element(o,id).is_some()){missing.push("检测到其他层数标题，层数证据冲突");}
                if missing.is_empty(){missing.push("页面证据冲突");}
                return Err(format!("悲鸣挑战前核验未通过：{}",missing.join("；")));}
            return Ok(Decision::Wait);
        }else{self.unverified_layer_frames=0;}
        if matches!(page,Page::NetworkError|Page::Defeat|Page::StaminaEmpty) {return Err(format!("{}：已停止，未自动确认提示",page.label()));}
        if self.page!=page {self.page=page;self.stable=1;self.page_since=now;} else {self.stable+=1;}
        if page==Page::Battle {self.battle_seen=true;}
        if page==Page::Reward {self.reward_seen=true;}
        if let Some(p)=&self.pending {
            if p.plan.expected.contains(&page) {
                self.pending=None;
            } else if now.saturating_sub(p.sent_at)>=p.plan.timeout_ms {
                return Err(format!("动作 {} 未在期限内得到预期页面确认（当前 {}）",p.plan.target,page.label()));
            } else {
                if page==p.plan.from && self.stable>=2 && now.saturating_sub(p.last_attempt)>=1800
                    && p.attempts<=p.plan.retry_limit
                    && let Some(target)=element(o,&p.plan.target)
                    && iou(target.bounds,p.plan.bounds)>0.65 {
                        let mut retry=p.plan.clone();retry.bounds=target.bounds;retry.point=target.center();
                        return Ok(Decision::Tap(retry));
                }
                return Ok(Decision::Wait);
            }
        }
        if page==Page::Challenge&&self.reward_seen&&self.stable>=2 {
            if self.owned_round&&self.battle_seen {self.completed+=1;}
            self.owned_round=false;self.battle_seen=false;self.reward_seen=false;
            if self.completed>=self.goal {return Ok(Decision::Complete);}
        }
        if self.completed>=self.goal {return Ok(Decision::Complete);}
        let limit=if page==Page::Unknown {10_000} else {self.timeout_ms};
        if now.saturating_sub(self.page_since)>limit {return Err(format!("{}超时，保留截图与状态轨迹",page.label()));}
        if self.stable<2 {return Ok(Decision::Wait);}
        let (target,expected,retry_limit)=match page {
            Page::Courtyard=>("explore",vec![Page::Explore],2),
            Page::Explore=>(if self.layer==11 {"soulbtn"}else{"soul_entry"},vec![Page::SoulMenu,Page::LayerSelect,Page::Challenge],2),
            Page::SoulMenu=>("orochi_entry",vec![Page::LayerSelect,Page::Challenge],2),
            Page::LayerSelect=>("layer1_option",vec![Page::Challenge],2),
            Page::Challenge=> {
                if self.owned_round {return Err("战斗未确认结算却返回挑战页，停止以避免重复消耗".into());}
                ("challenge",vec![Page::Prepare,Page::Battle,Page::Victory,Page::Reward],0)
            },
            Page::Prepare=>("ready",vec![Page::Battle,Page::Victory,Page::Reward],1),
            Page::Reward=>("continue",vec![Page::Challenge],2),
            _=>return Ok(Decision::Wait),
        };
        let Some(d)=element(o,target) else {return Ok(Decision::Wait)};
        Ok(Decision::Tap(Plan {from:page,target:target.into(),bounds:d.bounds,point:d.center(),expected,timeout_ms:12_000,retry_limit}))
    }
    // Only call after dispatch succeeded; a proposed/observe-only action never
    // creates progress or consumes a retry.
    pub fn dispatched(&mut self,plan:Plan,now:u64) {
        if plan.target=="challenge" {self.owned_round=true;self.battle_seen=false;self.reward_seen=false;}
        if let Some(p)=&mut self.pending { p.attempts+=1;p.plan=plan;p.last_attempt=now; /* deadline is never extended */ }
        else {self.pending=Some(Pending {plan,sent_at:now,last_attempt:now,attempts:1});}
    }
    pub fn interval_ms(&self)->u64 {if self.page==Page::Battle {1500} else {350}}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::detector::Timings;
    fn obs(ids:&[&str])->Observation {
        Observation {compute:crate::vision::runtime::Compute {provider:"CPU".into(),fallback_reason:None},dimensions:[1920,1080],timings:Timings::default(),detections:ids.iter().map(|id| {
            let (x,y)=match *id {
                "soul_title"=>(280.,55.),"orochi_title"=>(1545.,220.),"challenge"=>(1780.,940.),
                "layer1_selected"|"layer1_option"=>(240.,190.),"ready"=>(1760.,900.),
                "prepare_marker"|"continue"=>(960.,1040.),"auto_battle"=>(90.,990.),"battle_controls"=>(420.,50.),
                "reward"=>(960.,760.),_=>(960.,540.)};
            Detection {class:(*id).into(),confidence:0.95,bounds:[x-20.,y-20.,x+20.,y+20.]}
        }).collect()}
    }
    fn challenge()->Observation {obs(&["soul_title","orochi_title","challenge","layer1_selected"])}
    fn det(class:&str,confidence:f32,bounds:[f32;4])->Detection {
        Detection {class:class.into(),confidence,bounds}
    }
    fn challenge11()->Observation {obs(&["scene_orochi","layer11_selected","formation_locked","challenge"])}
    #[test]
    fn soul11_requires_exact_layer_and_lock_each_round() {
        assert_eq!(classify_soul11(&challenge11()),Page::Challenge);
        for absent in ["layer11_selected","formation_locked"] {
            let mut o=challenge11();o.detections.retain(|d|d.class!=absent);
            let mut c=Controller::soul11(2,180000);
            assert!(matches!(c.tick(&o,0).unwrap(),Decision::Wait));
            assert!(c.tick(&o,500).is_err());
        }
        let mut mixed=challenge11();mixed.detections.extend(obs(&["layer12_selected"]).detections);
        assert_eq!(classify_soul11(&mixed),Page::Unknown);
    }
    #[test]
    fn soul11_rejects_open_lock_even_when_closed_lock_is_also_reported() {
        let mut o=challenge11();o.detections.extend(obs(&["formation_unlocked"]).detections);
        assert_eq!(classify_soul11(&o),Page::Unknown);
        let mut c=Controller::soul11(1,180000);
        c.tick(&o,0).unwrap();
        assert!(c.tick(&o,500).unwrap_err().contains("开锁图标"));
    }
    #[test]
    fn soul11_completes_two_owned_rounds_and_stops_before_third_challenge() {
        let mut c=Controller::soul11(2,180000);let o=challenge11();
        c.tick(&o,0).unwrap();
        for round in 0..2 {
            let t=round*6000+500;
            let Decision::Tap(p)=c.tick(&o,t).unwrap() else {panic!()};
            assert_eq!(p.target,"challenge");assert_eq!(p.retry_limit,0);c.dispatched(p,t);
            c.tick(&obs(&["scene_fight"]),t+1000).unwrap();
            let reward=obs(&["scene_results","continue"]);
            c.tick(&reward,t+2000).unwrap();
            let Decision::Tap(p)=c.tick(&reward,t+2500).unwrap() else {panic!()};c.dispatched(p,t+2500);
            c.tick(&o,t+3000).unwrap();
            let decision=c.tick(&o,t+3500).unwrap();
            assert_eq!(c.completed,round as u32+1);
            if round==1 {assert!(matches!(decision,Decision::Complete));}
        }
    }
    #[test]
    fn soul11_adopted_reward_does_not_count_and_failure_never_clicks_continue() {
        let mut c=Controller::soul11(1,180000);
        c.tick(&obs(&["scene_results","continue"]),0).unwrap();
        c.tick(&challenge11(),1000).unwrap();c.tick(&challenge11(),1500).unwrap();assert_eq!(c.completed,0);
        assert!(c.tick(&obs(&["scene_fail","continue"]),2000).is_err());
    }
    #[test]
    fn explore_marker_just_below_action_threshold_still_identifies_the_page() {
        let o=Observation {compute:crate::vision::runtime::Compute {provider:"CPU".into(),fallback_reason:None},dimensions:[1920,1080],timings:Timings::default(),detections:vec![
            det("back",0.96,[6.4,7.6,123.2,109.8]),
            det("soul_entry",0.88,[569.8,203.6,627.1,453.0]),
            det("explore_marker",0.716,[1590.2,252.8,1909.8,307.1]),
        ]};
        assert_eq!(classify(&o),Page::Explore);
        assert!(element(&o,"explore_marker").is_some());
        let mut moved=o.clone();
        moved.detections.retain(|d|d.class!="explore_marker");
        assert_eq!(classify(&moved),Page::Explore);
        moved.detections.retain(|d|d.class!="back");
        assert_eq!(classify(&moved),Page::Unknown);
    }
    #[test]
    fn wrong_layer_missing_context_and_faults_never_challenge() {
        assert_eq!(classify(&obs(&["challenge"])),Page::Unknown);
        assert_eq!(classify(&obs(&["soul_title","orochi_title","challenge","layer1_option"])),Page::LayerSelect);
        let mut o=challenge();o.detections.extend(obs(&["network_error"]).detections);
        assert_eq!(classify(&o),Page::NetworkError);
        assert!(Controller::new(1,180000).tick(&o,0).is_err());
    }
    #[test]
    fn a_consuming_action_is_never_retried_and_has_deadline() {
        let mut c=Controller::new(1,180000);let o=challenge();
        c.tick(&o,0).unwrap();let Decision::Tap(p)=c.tick(&o,500).unwrap() else {panic!()};
        c.dispatched(p,500);
        assert!(matches!(c.tick(&o,3000).unwrap(),Decision::Wait));
        assert!(c.tick(&o,13000).is_err());
    }
    #[test]
    fn count_requires_owned_battle_reward_and_return_once() {
        let mut c=Controller::new(1,180000);let o=challenge();
        c.tick(&o,0).unwrap();let Decision::Tap(p)=c.tick(&o,500).unwrap() else {panic!()};c.dispatched(p,500);
        c.tick(&obs(&["auto_battle","battle_controls"]),1000).unwrap();
        let r=obs(&["reward","continue"]);c.tick(&r,2000).unwrap();
        let Decision::Tap(p)=c.tick(&r,2500).unwrap() else {panic!()};c.dispatched(p,2500);
        c.tick(&o,3000).unwrap();
        assert!(matches!(c.tick(&o,3500).unwrap(),Decision::Complete));assert_eq!(c.completed,1);
        let mut adopted=Controller::new(1,180000);
        adopted.tick(&r,0).unwrap();adopted.tick(&o,1000).unwrap();assert_eq!(adopted.completed,0);
    }
}
