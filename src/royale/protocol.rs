//! Tank Royale WebSocket messages (JSON, see the official `schema/schemas`).

use super::Setup;
use super::battle::{Bot, BulletState, Event, Intent, Score};
use serde::Deserialize;
use serde_json::{Value, json};

/// Version announced to bots: the Bot APIs accept servers of the same major version.
pub const PROTOCOL_VERSION: &str = "1.4.0";

/// Messages a bot sends.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum FromBot {
    BotHandshake(BotHandshake),
    BotReady,
    BotIntent(BotIntent),
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BotHandshake {
    pub session_id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub platform: Option<String>,
    pub programming_lang: Option<String>,
    pub is_droid: bool,
    pub team_id: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BotIntent {
    pub turn_rate: Option<f64>,
    pub gun_turn_rate: Option<f64>,
    pub radar_turn_rate: Option<f64>,
    pub target_speed: Option<f64>,
    pub firepower: Option<f64>,
    pub adjust_gun_for_body_turn: Option<bool>,
    pub adjust_radar_for_body_turn: Option<bool>,
    pub adjust_radar_for_gun_turn: Option<bool>,
    pub rescan: Option<bool>,
    pub fire_assist: Option<bool>,
    pub body_color: Option<String>,
    pub turret_color: Option<String>,
    pub radar_color: Option<String>,
    pub bullet_color: Option<String>,
    pub std_out: Option<String>,
    pub std_err: Option<String>,
}

impl BotIntent {
    /// The server's mapping: missing fields take their defaults.
    pub fn to_intent(&self) -> Intent {
        let d = Intent::default();
        Intent {
            target_speed: self.target_speed.unwrap_or(d.target_speed),
            turn_rate: self.turn_rate.unwrap_or(d.turn_rate),
            gun_turn_rate: self.gun_turn_rate.unwrap_or(d.gun_turn_rate),
            radar_turn_rate: self.radar_turn_rate.unwrap_or(d.radar_turn_rate),
            firepower: self.firepower.unwrap_or(d.firepower),
            adjust_gun_for_body_turn: self.adjust_gun_for_body_turn.unwrap_or(false),
            adjust_radar_for_body_turn: self.adjust_radar_for_body_turn.unwrap_or(false),
            adjust_radar_for_gun_turn: self.adjust_radar_for_gun_turn.unwrap_or(false),
            rescan: self.rescan.unwrap_or(false),
            fire_assist: self.fire_assist.unwrap_or(d.fire_assist),
        }
    }

    /// Later intents in the same turn only change the fields they carry.
    pub fn merge(&mut self, o: BotIntent) {
        macro_rules! take {
            ($($f:ident),*) => {$( if o.$f.is_some() { self.$f = o.$f; } )*};
        }
        take!(
            turn_rate,
            gun_turn_rate,
            radar_turn_rate,
            target_speed,
            firepower,
            adjust_gun_for_body_turn,
            adjust_radar_for_body_turn,
            adjust_radar_for_gun_turn,
            rescan,
            fire_assist,
            body_color,
            turret_color,
            radar_color,
            bullet_color,
            std_out,
            std_err
        );
    }
}

pub fn server_handshake(session_id: &str, setup: &Setup) -> Value {
    json!({
        "type": "ServerHandshake",
        "sessionId": session_id,
        "name": "Claude Tanks",
        "variant": "Tank Royale",
        "version": PROTOCOL_VERSION,
        "behaviorVersion": 2,
        "gameTypes": ["classic", "1v1"],
        "gameSetup": game_setup(setup, 2),
        "features": {"debugMode": false, "breakpointMode": false},
    })
}

pub fn game_setup(s: &Setup, participants: usize) -> Value {
    json!({
        "gameType": "classic",
        "arenaWidth": s.arena_width,
        "isArenaWidthLocked": true,
        "arenaHeight": s.arena_height,
        "isArenaHeightLocked": true,
        "minNumberOfParticipants": participants,
        "isMinNumberOfParticipantsLocked": true,
        "maxNumberOfParticipants": participants,
        "isMaxNumberOfParticipantsLocked": true,
        "numberOfRounds": s.rounds,
        "isNumberOfRoundsLocked": true,
        "gunCoolingRate": s.gun_cooling_rate,
        "isGunCoolingRateLocked": true,
        "maxInactivityTurns": s.max_inactivity_turns,
        "isMaxInactivityTurnsLocked": true,
        "turnTimeout": s.turn_timeout_us,
        "isTurnTimeoutLocked": true,
        "readyTimeout": s.ready_timeout_us,
        "isReadyTimeoutLocked": true,
        "defaultTurnsPerSecond": 30,
    })
}

pub fn game_started(my_id: u32, start: Option<&Bot>, names: &[String], setup: &Setup) -> Value {
    let mut v = json!({
        "type": "GameStartedEventForBot",
        "myId": my_id,
        "teammateIds": [],
        "botNames": names.iter().enumerate()
            .map(|(i, n)| json!({"botId": i + 1, "name": n}))
            .collect::<Vec<_>>(),
        "gameSetup": game_setup(setup, names.len()),
    });
    if let Some(b) = start {
        v["startX"] = json!(b.x);
        v["startY"] = json!(b.y);
        v["startDirection"] = json!(b.direction);
    }
    v
}

pub fn round_started(round: u32) -> Value {
    json!({"type": "RoundStartedEvent", "roundNumber": round})
}

pub fn skipped_turn(turn: u32) -> Value {
    json!({"type": "SkippedTurnEvent", "turnNumber": turn})
}

pub fn bullet_state(b: &BulletState) -> Value {
    json!({
        "bulletId": b.id,
        "ownerId": b.owner,
        "power": b.power,
        "x": b.x,
        "y": b.y,
        "direction": b.direction,
    })
}

pub fn event(e: &Event, turn: u32) -> Value {
    match e {
        Event::BotDeath { victim } => {
            json!({"type": "BotDeathEvent", "turnNumber": turn, "victimId": victim})
        }
        Event::BotHitBot {
            bot,
            victim,
            energy,
            x,
            y,
            rammed,
        } => json!({
            "type": "BotHitBotEvent", "turnNumber": turn, "botId": bot, "victimId": victim,
            "energy": energy, "x": x, "y": y, "rammed": rammed,
        }),
        Event::BotHitWall { victim } => {
            json!({"type": "BotHitWallEvent", "turnNumber": turn, "victimId": victim})
        }
        Event::BulletFired { bullet } => json!({
            "type": "BulletFiredEvent", "turnNumber": turn, "bullet": bullet_state(bullet),
        }),
        Event::BulletHitBot {
            bullet,
            victim,
            damage,
            energy,
        } => json!({
            "type": "BulletHitBotEvent", "turnNumber": turn, "victimId": victim,
            "bullet": bullet_state(bullet), "damage": damage, "energy": energy,
        }),
        Event::BulletHitBullet { bullet, hit_bullet } => json!({
            "type": "BulletHitBulletEvent", "turnNumber": turn,
            "bullet": bullet_state(bullet), "hitBullet": bullet_state(hit_bullet),
        }),
        Event::BulletHitWall { bullet } => json!({
            "type": "BulletHitWallEvent", "turnNumber": turn, "bullet": bullet_state(bullet),
        }),
        Event::ScannedBot {
            by,
            scanned,
            energy,
            x,
            y,
            direction,
            speed,
        } => json!({
            "type": "ScannedBotEvent", "turnNumber": turn, "scannedByBotId": by,
            "scannedBotId": scanned, "energy": energy, "x": x, "y": y,
            "direction": direction, "speed": speed,
        }),
        Event::WonRound => json!({"type": "WonRoundEvent", "turnNumber": turn}),
    }
}

pub fn tick(
    round: u32,
    turn: u32,
    bot: &Bot,
    enemies: usize,
    bullets: &[BulletState],
    events: &[Event],
) -> Value {
    json!({
        "type": "TickEventForBot",
        "roundNumber": round,
        "turnNumber": turn,
        "botState": {
            "isDroid": false,
            "energy": bot.energy,
            "x": bot.x,
            "y": bot.y,
            "direction": bot.direction,
            "gunDirection": bot.gun_direction,
            "radarDirection": bot.radar_direction,
            "radarSweep": bot.radar_sweep,
            "speed": bot.speed,
            "turnRate": bot.turn_rate,
            "gunTurnRate": bot.gun_turn_rate,
            "radarTurnRate": bot.radar_turn_rate,
            "gunHeat": bot.gun_heat,
            "enemyCount": enemies,
            "isDebuggingEnabled": false,
        },
        "bulletStates": bullets.iter().map(bullet_state).collect::<Vec<_>>(),
        "events": events.iter().map(|e| event(e, turn)).collect::<Vec<_>>(),
    })
}

pub fn results(score: &Score, rank: usize) -> Value {
    let r = |v: f64| v.round() as i64;
    json!({
        "rank": rank,
        "survival": r(score.survival),
        "lastSurvivorBonus": r(score.last_survivor_bonus),
        "bulletDamage": r(score.bullet_damage),
        "bulletKillBonus": r(score.bullet_kill_bonus),
        "ramDamage": r(score.ram_damage),
        "ramKillBonus": r(score.ram_kill_bonus),
        "totalScore": r(score.total()),
        "firstPlaces": score.first_places,
        "secondPlaces": score.second_places,
        "thirdPlaces": score.third_places,
    })
}

pub fn round_ended(round: u32, turn: u32, score: &Score, rank: usize) -> Value {
    json!({
        "type": "RoundEndedEventForBot",
        "roundNumber": round,
        "turnNumber": turn,
        "results": results(score, rank),
    })
}

pub fn game_ended(rounds: u32, score: &Score, rank: usize) -> Value {
    json!({
        "type": "GameEndedEventForBot",
        "numberOfRounds": rounds,
        "results": results(score, rank),
    })
}

pub fn game_aborted() -> Value {
    json!({"type": "GameAbortedEvent"})
}
