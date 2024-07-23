use serde::{Deserialize, Serialize};

/// LeaderElector is a struct that is used to elect a leader among a group of robots using the token ring algorithm.
/// It checks both the ID and the backup status of the robots to choose the leader.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct LeaderElector {
    pub my_id: usize,
    pub valid_backup: bool,
}

impl LeaderElector {
    pub fn new(my_id: usize) -> Self {
        Self {
            my_id,
            valid_backup: false,
        }
    }

    /// Choose the leader among the candidates, based on the ID and the backup status of the robots.
    pub fn choose_leader(&mut self, candidates: Vec<(usize, bool)>) -> usize {
        let mut new_leader_id = self.my_id;
        let mut has_valid_backup = self.valid_backup;

        for (candidate, valid_backup) in candidates {
            if valid_backup && ((candidate > new_leader_id) || !has_valid_backup) {
                new_leader_id = candidate;
                has_valid_backup = true;
            }
        }

        new_leader_id
    }

    /// Start the election process by adding the current robot to the candidates list.
    pub fn start_election(&mut self) -> Vec<(usize, bool)> {
        let new_candidates = vec![(self.my_id, self.valid_backup)];
        new_candidates
    }

    /// Add the current robot to the candidates list.
    pub fn add_candidate(&mut self, mut candidates: Vec<(usize, bool)>) -> Vec<(usize, bool)> {
        candidates.push((self.my_id, self.valid_backup));
        candidates
    }

    /// Check if the round is finished by checking if the current robot is the leader.
    pub fn check_round_finished(&self, candidates: Vec<(usize, bool)>) -> bool {
        for (candidate, _) in candidates {
            if candidate == self.my_id {
                return true;
            }
        }
        false
    }

    /// Validate the backup of the current robot.
    pub fn validate_backup(&mut self) {
        self.valid_backup = true;
    }
}
