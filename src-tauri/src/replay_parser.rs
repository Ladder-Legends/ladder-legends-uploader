use std::collections::HashMap;
use std::path::Path;

/// Game type classification for SC2 replays
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameType {
    /// 1v1 ladder (ranked matchmaking)
    Ladder1v1,
    /// 1v1 unranked matchmaking
    Unranked1v1,
    /// 1v1 private/custom game
    Private1v1,
    /// 1v1 with observer(s)
    Obs1v1,
    /// 1v1 vs AI
    VsAI1v1,

    /// 2v2 ladder (ranked matchmaking)
    Ladder2v2,
    /// 2v2 unranked matchmaking
    Unranked2v2,
    /// 2v2 private/custom game
    Private2v2,
    /// 2v2 with observer(s)
    Obs2v2,

    /// 3v3 or higher team games
    TeamGame,
    /// Arcade/custom games
    Arcade,
    /// Practice/training mode
    Practice,
    /// Other/unknown
    Other,
}

impl GameType {
    /// Convert to string representation for storage/display
    pub fn as_str(&self) -> &str {
        match self {
            GameType::Ladder1v1 => "1v1-ladder",
            GameType::Unranked1v1 => "1v1-unranked",
            GameType::Private1v1 => "1v1-private",
            GameType::Obs1v1 => "1v1-obs",
            GameType::VsAI1v1 => "1vAI",
            GameType::Ladder2v2 => "2v2-ladder",
            GameType::Unranked2v2 => "2v2-unranked",
            GameType::Private2v2 => "2v2-private",
            GameType::Obs2v2 => "2v2-obs",
            GameType::TeamGame => "team-game",
            GameType::Arcade => "arcade",
            GameType::Practice => "practice",
            GameType::Other => "other",
        }
    }

    /// Check if this game type should be uploaded
    pub fn should_upload(&self) -> bool {
        matches!(
            self,
            GameType::Ladder1v1
                | GameType::Unranked1v1
                | GameType::Private1v1
                | GameType::Obs1v1
                | GameType::Ladder2v2
                | GameType::Unranked2v2
                | GameType::Private2v2
                | GameType::Obs2v2
        )
    }
}

/// Parse replay and extract game type information
pub fn get_game_type(file_path: &Path) -> Result<GameType, String> {
    // Parse MPQ archive using s2protocol (takes file path, not bytes)
    let file_path_str = file_path.to_str().ok_or("Invalid file path")?;
    let (mpq, file_contents) = s2protocol::read_mpq(file_path_str)
        .map_err(|e| format!("Failed to parse MPQ: {:?}", e))?;

    // Read the details which contains player/team information
    let details = s2protocol::versions::read_details(
        file_path_str,
        &mpq,
        &file_contents,
    )
    .map_err(|e| format!("Failed to read details: {:?}", e))?;

    // Read init data which contains game mode flags
    let init_data = s2protocol::versions::read_init_data(
        file_path_str,
        &mpq,
        &file_contents,
    )
    .map_err(|e| format!("Failed to read init data: {:?}", e))?;

    // Get game options from lobby state
    let game_options = &init_data.sync_lobby_state.game_description.game_options;

    // Count players per team and classify by type
    let mut teams: HashMap<u8, usize> = HashMap::new(); // team_id -> human_count
    let mut observer_count = 0;
    let mut ai_count = 0;

    for player in &details.player_list {
        // observe: 0 = participant, 1+ = observer
        if player.observe != 0 {
            observer_count += 1;
            continue;
        }

        // control: 2 = Human, 3 = Computer/AI
        match player.control {
            2 => {
                // Human player - add to team
                *teams.entry(player.team_id).or_insert(0) += 1;
            }
            3 => {
                // AI player
                ai_count += 1;
            }
            _ => {}
        }
    }

    // Convert teams to sorted list of team sizes
    let mut team_sizes: Vec<usize> = teams.values().copied().collect();
    team_sizes.sort_by(|a, b| b.cmp(a)); // Sort descending
    let total_human_players: usize = team_sizes.iter().sum();

    // Classify the game type based on all available information
    let game_type = classify_game_type(
        &team_sizes,
        total_human_players,
        observer_count,
        ai_count,
        game_options.amm,
        game_options.competitive,
        game_options.practice,
    );

    Ok(game_type)
}

fn classify_game_type(
    team_sizes: &[usize],
    total_humans: usize,
    observers: usize,
    ai_count: usize,
    amm: bool,
    competitive: bool,
    practice: bool,
) -> GameType {
    // Practice mode
    if practice {
        return GameType::Practice;
    }

    // Check for 1v1 games
    if team_sizes.len() == 2 && team_sizes[0] == 1 && team_sizes[1] == 1 {
        // 1v1 human vs human
        if observers > 0 {
            return GameType::Obs1v1;
        }
        if ai_count > 0 {
            return GameType::VsAI1v1;
        }
        // Determine if ladder, unranked, or private
        if amm {
            return if competitive {
                GameType::Ladder1v1
            } else {
                GameType::Unranked1v1
            };
        }
        return GameType::Private1v1;
    }

    // Check for 1vAI (single human vs AI)
    if team_sizes.len() == 1 && team_sizes[0] == 1 && ai_count > 0 {
        return GameType::VsAI1v1;
    }

    // Check for 2v2 games
    if team_sizes.len() == 2 && team_sizes[0] == 2 && team_sizes[1] == 2 {
        if observers > 0 {
            return GameType::Obs2v2;
        }
        // Determine if ladder, unranked, or private
        if amm {
            return if competitive {
                GameType::Ladder2v2
            } else {
                GameType::Unranked2v2
            };
        }
        return GameType::Private2v2;
    }

    // 3v3 or larger team games
    if team_sizes.len() == 2 && total_humans >= 6 {
        return GameType::TeamGame;
    }

    // Arcade/custom games (unusual team configurations)
    if !amm && !practice && total_humans > 0 {
        return GameType::Arcade;
    }

    GameType::Other
}

/// Check if a replay should be uploaded (legacy compatibility function)
///
/// This filters for competitive 1v1 and 2v2 games, excluding:
/// - Practice games
/// - Arcade games
/// - AI games
/// - Team games (3v3+)
pub fn is_1v1_replay(file_path: &Path) -> Result<bool, String> {
    let game_type = get_game_type(file_path)?;
    Ok(game_type.should_upload())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_practice_aim_is_not_1v1() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/practice-aim.SC2Replay");

        if replay_path.exists() {
            let result = is_1v1_replay(&replay_path);
            match result {
                Ok(is_1v1) => {
                    assert_eq!(is_1v1, false, "Practice aim should not be detected as 1v1");
                }
                Err(e) => panic!("Failed to parse practice aim replay: {}", e),
            }

            // Also test game type classification
            let game_type = get_game_type(&replay_path).unwrap();
            println!("Practice game type: {:?} ({})", game_type, game_type.as_str());
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }

    #[test]
    fn test_ladder_game_is_1v1() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/1v1-ladder.SC2Replay");

        if replay_path.exists() {
            let result = is_1v1_replay(&replay_path);
            match result {
                Ok(is_1v1) => {
                    assert_eq!(is_1v1, true, "Ladder game should be detected as 1v1");
                }
                Err(e) => panic!("Failed to parse ladder replay: {}", e),
            }

            // Also test game type classification
            let game_type = get_game_type(&replay_path).unwrap();
            println!("Ladder game type: {:?} ({})", game_type, game_type.as_str());
            assert!(game_type.should_upload(), "Ladder game should be uploadable");
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }
}
