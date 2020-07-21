use {
    argh::{self, FromArgs},
    csv,
    serde::Deserialize,
    std::cmp::Ordering,
    std::collections::{HashMap, HashSet},
};

#[derive(Debug, Deserialize)]
struct RawPlayer {
    name: String,
    wins: u8,
    losses: u8,
    opp1: String,
    opp2: String,
    opp3: String,
    opp4: String,
    opp_wins: u8,
    opp_losses: u8,
}

#[derive(Clone, Debug)]
struct Player {
    name: String,
    wins: u8,
    losses: u8,
    opp_wins: u8,
    opp_losses: u8,
    opponents: Vec<String>,
    placements: HashMap<usize, usize>,
}

impl Player {
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

fn rank_players(p1: &Player, p2: &Player) -> Ordering {
    let p1_oppwr = opponent_winrate(p1);
    let p2_oppwr = opponent_winrate(p2);
    p1.wins
        .cmp(&p2.wins)
        .then(p1_oppwr.partial_cmp(&p2_oppwr).unwrap())
}

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

fn read_matches(rdr: impl std::io::Read) -> Vec<(String, String)> {
    let mut matches = HashSet::new();
    let mut reader = csv::Reader::from_reader(rdr);
    for row in reader.deserialize() {
        let match_: Match = row.unwrap();
        let player1 = strip_prefix(match_.player1, 3);
        let player2 = strip_prefix(match_.player2, 3);
        if player1.cmp(&player2) == Ordering::Greater {
            matches.insert((player1, player2));
        }
    }
    matches.into_iter().collect()
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

    for i in 0..(1 << matches.len()) {
        simulate(i, &matches, &mut players);
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
        Box::new(std::fs::File::open(file).unwrap())
    } else {
        Box::new(std::io::stdout())
    };
    write_results(&top8, output);
}

fn simulate(i: usize, matches: &Vec<(String, String)>, players: &mut HashMap<String, Player>) {
    let mut players_copy = players.clone();
    for (matchnum, matchplayers) in matches.iter().enumerate() {
        let (winner, loser) = if i & (1 << matchnum) == 0 {
            (&matchplayers.0, &matchplayers.1)
        } else {
            (&matchplayers.1, &matchplayers.0)
        };

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
    ranking.sort_by(|p1, p2| rank_players(p1, p2).reverse());
    for (rank, player) in ranking.iter().enumerate().take(8) {
        players.entry(player.name.clone()).and_modify(|e| {
            e.add_placement(rank + 1);
        });
    }
    if i % 10000 == 0 {
        println!("iteration: {}", i);
    }
}
