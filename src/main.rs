use {
    argh::{self, FromArgs},
    csv,
    serde::Deserialize,
    std::cmp::Ordering,
    std::collections::{HashMap, HashSet},
};

#[derive(Debug, Deserialize)]
/// Player data loaded from CSV
struct RawPlayer {
    /// name of the player
    name: String,
    /// number of wins by the player
    wins: u8,
    /// number of losses by the player
    losses: u8,
    /// first opponent
    opp1: String,
    /// second opponent
    opp2: String,
    /// third opponent
    opp3: String,
    /// fourth opponent
    opp4: String,
    /// total wins by all opponents, excluding wins against the player
    opp_wins: u8,
    /// total losses by all opponents, excluding losses against the player
    opp_losses: u8,
}

#[derive(Clone, Debug)]
/// Internal representation of a Player
struct Player {
    /// name of the player
    name: String,
    /// number of wins by the player
    wins: u8,
    /// number of losses by the player
    losses: u8,
    /// total wins by all opponents, excluding wins against the player
    opp_wins: u8,
    /// total losses by all opponents, excluding losses against the player
    opp_losses: u8,
    /// list of opponents
    opponents: Vec<String>,
    /// counts of placements by the player, keyed by rank
    placements: HashMap<usize, usize>,
}

impl Player {
    /// create a new Player from a RawPlayer
    fn new(data: RawPlayer) -> Player {
        Player {
            name: data.name,
            wins: data.wins,
            losses: data.losses,
            opp_wins: data.opp_wins,
            opp_losses: data.opp_losses,
            opponents: vec![data.opp1, data.opp2, data.opp3, data.opp4],
            placements: HashMap::new(),
        }
    }

    /// add a final placement for the player
    fn add_placement(&mut self, place: usize) {
        *self.placements.entry(place).or_insert(0) += 1;
    }

    fn add_win(&mut self) {
        self.wins += 1;
    }

    fn add_loss(&mut self) {
        self.losses += 1;
    }

    fn add_opponent_win(&mut self) {
        self.opp_wins += 1;
    }

    fn add_opponent_loss(&mut self) {
        self.opp_losses += 1;
    }
}

#[derive(Debug, Deserialize)]
/// A match between two players
struct Match {
    player1: String,
    player2: String,
}

#[derive(Debug, FromArgs)]
/// CC Listener Series simulator
struct Opts {
    #[argh(option, short = 'p')]
    /// filename with CSV data of players
    players: String,

    #[argh(option, short = 'm')]
    /// filename with CSV data of matches
    matches: String,

    #[argh(option, short = 'o')]
    /// filename for writing output (default: stdout)
    output: Option<String>,

    #[argh(option, short = 'n')]
    /// number of simulations to run (default: all)
    simulation_count: Option<usize>,

    #[argh(option, short = 't')]
    /// number of top ranks to compute in each simulation
    top_ranks: usize,
}

fn strip_prefix(s: String, prefix_length: usize) -> String {
    if s.len() >= prefix_length {
        String::from(&s[prefix_length..])
    } else {
        s
    }
}

fn opponent_winrate(p: &Player) -> f32 {
    (p.opp_wins as f32) / ((p.opp_wins + p.opp_losses) as f32)
}

/// Order players first by totals wins, then by opponent winrate
fn rank_players(p1: &Player, p2: &Player) -> Ordering {
    let p1_oppwr = opponent_winrate(p1);
    let p2_oppwr = opponent_winrate(p2);
    p1.wins
        .cmp(&p2.wins)
        .then(p1_oppwr.partial_cmp(&p2_oppwr).unwrap())
}

/// Read in player data from `rdr`.
///
/// Returns a map of Player data keyed by player name.
fn read_players(rdr: impl std::io::Read) -> HashMap<String, Player> {
    let mut players = HashMap::new();
    let mut reader = csv::Reader::from_reader(rdr);
    for row in reader.deserialize() {
        let mut player: RawPlayer = row.unwrap();
        player.name = strip_prefix(player.name, 3);
        player.opp1 = strip_prefix(player.opp1, 3);
        player.opp2 = strip_prefix(player.opp2, 3);
        player.opp3 = strip_prefix(player.opp3, 3);
        player.opp4 = strip_prefix(player.opp4, 3);
        players.insert(player.name.clone(), Player::new(player));
    }
    players
}

/// Read in match data from `rdr`.
///
/// Match data may contain duplicates, e.g. with opponents swapped.
/// Returns a vector of matches, with duplicates removed.
fn read_matches(rdr: impl std::io::Read) -> Vec<(String, String)> {
    let mut matches = HashSet::new();
    let mut reader = csv::Reader::from_reader(rdr);
    for row in reader.deserialize() {
        let match_: Match = row.unwrap();
        let player1 = strip_prefix(match_.player1, 3);
        let player2 = strip_prefix(match_.player2, 3);
        matches.insert(if player1.cmp(&player2) == Ordering::Greater {
            (player1, player2)
        } else {
            (player2, player1)
        });
    }
    // Sort the matches to get deterministic simulations when a subset of simulations are run.
    let mut result = matches.into_iter().collect::<Vec<_>>();
    result.sort_unstable();
    result
}

fn write_results(players: &Vec<&Player>, mut w: Box<dyn std::io::Write>) {
    write!(w, "final players:\n").unwrap();
    for player in players {
        if player.placements.len() > 0 {
            write!(w, "  {}: {:?}\n", player.name, player.placements).unwrap();
        }
    }
}

fn main() {
    let opts: Opts = argh::from_env();

    let player_file = std::fs::File::open(opts.players).unwrap();
    let mut players = read_players(player_file);

    let match_file = std::fs::File::open(opts.matches).unwrap();
    let matches = read_matches(match_file);

    let simulations = std::cmp::min(
        1 << matches.len(),
        opts.simulation_count.unwrap_or(std::usize::MAX),
    );
    for i in 0..simulations {
        simulate(i, opts.top_ranks, &matches, &mut players);
    }

    let top8 = players
        .iter()
        .filter_map(|p| {
            if p.1.placements.len() > 0 {
                Some(p.1)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let output: Box<dyn std::io::Write> = if let Some(file) = opts.output {
        Box::new(std::fs::File::create(file).unwrap())
    } else {
        Box::new(std::io::stdout())
    };
    write_results(&top8, output);
}

fn simulate(
    iteration: usize,
    top_ranks: usize,
    matches: &Vec<(String, String)>,
    players: &mut HashMap<String, Player>,
) {
    let mut players_copy = players.clone();
    for (matchnum, matchplayers) in matches.iter().enumerate() {
        let (winner, loser) = if iteration & (1 << matchnum) == 0 {
            (&matchplayers.0, &matchplayers.1)
        } else {
            (&matchplayers.1, &matchplayers.0)
        };

        /// XXX: use information about number of opponents instead of hardcoding to 4
        let mut opp_wins = Vec::with_capacity(4);
        players_copy.entry(winner.to_string()).and_modify(|e| {
            e.add_win();
            opp_wins.extend(e.opponents.iter().cloned());
        });
        for opponent in opp_wins {
            players_copy
                .entry(opponent)
                .and_modify(Player::add_opponent_win);
        }

        let mut opp_losses = Vec::with_capacity(4);
        players_copy.entry(loser.to_string()).and_modify(|e| {
            e.add_loss();
            opp_losses.extend(e.opponents.iter().cloned());
        });
        for opponent in opp_losses {
            players_copy
                .entry(opponent)
                .and_modify(Player::add_opponent_loss);
        }
    }
    let mut ranking: Vec<_> = players_copy.values().collect();
    // Reverse the sort to get highest win total first
    ranking.sort_by(|p1, p2| rank_players(p1, p2).reverse());
    for (rank, player) in ranking.iter().enumerate().take(top_ranks) {
        players.entry(player.name.clone()).and_modify(|e| {
            e.add_placement(rank + 1);
        });
    }
    if iteration % 10000 == 0 {
        println!("iteration: {}", iteration);
    }
}
