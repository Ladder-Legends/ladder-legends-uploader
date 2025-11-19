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

/// Player information from a replay
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    pub name: String,
    pub is_observer: bool,
}

/// Extract player names from a replay
/// Returns list of all players with their observer status
pub fn get_players(file_path: &Path) -> Result<Vec<PlayerInfo>, String> {
    // Parse MPQ archive using s2protocol
    let file_path_str = file_path.to_str().ok_or("Invalid file path")?;
    let (mpq, file_contents) = s2protocol::read_mpq(file_path_str)
        .map_err(|e| format!("Failed to parse MPQ: {:?}", e))?;

    // Read the details which contains player information
    let details = s2protocol::versions::read_details(
        file_path_str,
        &mpq,
        &file_contents,
    )
    .map_err(|e| format!("Failed to read details: {:?}", e))?;

    let mut players = Vec::new();

    for player in &details.player_list {
        // Skip AI players (control: 3)
        if player.control == 3 {
            continue;
        }

        players.push(PlayerInfo {
            name: player.name.clone(),
            is_observer: player.observe != 0,
        });
    }

    Ok(players)
}

/// Check if a replay contains any of the given player names as active players (not observers)
/// Returns true if at least one name matches an active player
pub fn contains_active_player(file_path: &Path, player_names: &[String]) -> Result<bool, String> {
    if player_names.is_empty() {
        // No names to check - allow all replays
        return Ok(true);
    }

    let players = get_players(file_path)?;

    // Check if any of the given names appear as active players
    for player in players {
        if !player.is_observer && player_names.contains(&player.name) {
            return Ok(true);
        }
    }

    Ok(false)
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

    #[test]
    fn test_get_players_extracts_names_and_observer_status() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/1v1-ladder.SC2Replay");

        if replay_path.exists() {
            let players = get_players(&replay_path).expect("Should extract players");

            assert!(!players.is_empty(), "Should find at least one player");

            // In a 1v1 ladder game, we expect 2 active players (no observers)
            let active_players: Vec<_> = players.iter().filter(|p| !p.is_observer).collect();
            assert_eq!(active_players.len(), 2, "Should have exactly 2 active players in 1v1 game");

            // All players should have names
            for player in &players {
                assert!(!player.name.is_empty(), "Player name should not be empty");
            }

            println!("Players found:");
            for player in &players {
                println!("  - {} (observer: {})", player.name, player.is_observer);
            }
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }

    #[test]
    fn test_contains_active_player_finds_player() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/1v1-ladder.SC2Replay");

        if replay_path.exists() {
            // First, get actual player names
            let players = get_players(&replay_path).expect("Should extract players");
            let active_players: Vec<_> = players.iter()
                .filter(|p| !p.is_observer)
                .map(|p| p.name.clone())
                .collect();

            assert!(!active_players.is_empty(), "Need at least one active player for test");

            // Test with first player's name
            let test_name = active_players[0].clone();
            let result = contains_active_player(&replay_path, &[test_name.clone()])
                .expect("Should check players");

            assert!(result, "Should find player '{}' as active in game", test_name);

            println!("✅ Successfully found player '{}' as active", test_name);
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }

    #[test]
    fn test_contains_active_player_does_not_find_nonexistent_player() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/1v1-ladder.SC2Replay");

        if replay_path.exists() {
            let fake_names = vec!["NonExistentPlayer123".to_string()];
            let result = contains_active_player(&replay_path, &fake_names)
                .expect("Should check players");

            assert!(!result, "Should not find fake player name in game");

            println!("✅ Correctly rejected fake player name");
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }

    #[test]
    fn test_contains_active_player_with_multiple_names() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/1v1-ladder.SC2Replay");

        if replay_path.exists() {
            // Get actual player names
            let players = get_players(&replay_path).expect("Should extract players");
            let active_players: Vec<_> = players.iter()
                .filter(|p| !p.is_observer)
                .map(|p| p.name.clone())
                .collect();

            if !active_players.is_empty() {
                // Test with mix of real and fake names
                let mut test_names = vec!["FakePlayer1".to_string(), "FakePlayer2".to_string()];
                test_names.push(active_players[0].clone());

                let result = contains_active_player(&replay_path, &test_names)
                    .expect("Should check players");

                assert!(result, "Should find at least one matching player from list");

                println!("✅ Found player from list of {} names", test_names.len());
            }
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }

    #[test]
    fn test_contains_active_player_with_empty_list_allows_all() {
        let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_replays/1v1-ladder.SC2Replay");

        if replay_path.exists() {
            let empty_names: Vec<String> = vec![];
            let result = contains_active_player(&replay_path, &empty_names)
                .expect("Should check players");

            assert!(result, "Empty player list should allow all replays");

            println!("✅ Empty list correctly allows all replays");
        } else {
            println!("Skipping test - replay file not found: {:?}", replay_path);
        }
    }

    #[test]
    fn test_player_info_equality() {
        let player1 = PlayerInfo {
            name: "TestPlayer".to_string(),
            is_observer: false,
        };

        let player2 = PlayerInfo {
            name: "TestPlayer".to_string(),
            is_observer: false,
        };

        let player3 = PlayerInfo {
            name: "TestPlayer".to_string(),
            is_observer: true, // Different observer status
        };

        assert_eq!(player1, player2, "Players with same data should be equal");
        assert_ne!(player1, player3, "Players with different observer status should not be equal");
    }

    #[test]
    fn test_player_info_debug() {
        let player = PlayerInfo {
            name: "TestPlayer".to_string(),
            is_observer: false,
        };

        let debug_str = format!("{:?}", player);
        assert!(debug_str.contains("TestPlayer"));
        assert!(debug_str.contains("false"));
    }
}
