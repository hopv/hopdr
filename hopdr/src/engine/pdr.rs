use std::{unimplemented};
use super::{Problem, VerificationResult};
use super::rtype::Environment;


enum PDRResult {
    Valid,
    Invalid,
    Unknown
}

struct Candidate {
    env: Environment,
    index: u64,
}

struct HoPDR<'a> {
    models: Vec<Candidate>,
    expand_cnt: u64,
    envs: Vec<Environment>,
    problem: &'a Problem,
}


enum RefuteOrCex<A, B> {
    Refutable(A),
    Cex(B),
}

impl<'a> HoPDR<'a> {
    fn new(problem: &'a Problem) -> HoPDR<'a> {
        let mut hopdr = HoPDR{models: Vec::new(), expand_cnt: 0, envs: Vec::new(), problem};
        hopdr.initialize();
        hopdr
    }

    fn check_valid(&self) -> Option<Candidate> {
        unimplemented!()
    }

    fn check_inductive(&self) -> bool {
        unimplemented!()
    }

    fn initialize(&mut self) {

    }

    fn unfold(&mut self) {
        self.expand_cnt += 1;
        let mut new_env = Environment::new();
        for c in self.problem.clauses.iter() {
            new_env.add_top(c.head.id, &c.head.ty)
        }
    }

    fn valid(&mut self) -> PDRResult {
        unimplemented!()
    }

    fn candidate(&mut self, _c: Candidate) {

    }

    fn is_refutable(&self, _c: &Candidate) -> RefuteOrCex<Environment, Candidate> {
        unimplemented!()
    }

    fn check_feasible(&mut self) -> PDRResult {
        loop {
            match self.models.pop() {
                Some(c) => {
                    match self.is_refutable(&c) {
                        RefuteOrCex::Refutable(env) => {
                            self.conflict(c, env);
                        },
                        RefuteOrCex::Cex(c2) => {
                            self.models.push(c);
                            self.decide(c2);
                        }
                    }
                },
                None => { return PDRResult::Unknown }
            }
        }
    }

    fn conflict(&mut self, _candidate: Candidate, _refute_env: Environment) {

    }

    fn decide(&mut self, candidate: Candidate) {
        self.models.push(candidate);
    }

    fn run(&mut self) -> PDRResult {
        loop {
            match self.check_valid() {
                Some(candidate) => {
                    self.candidate(candidate);
                    self.check_feasible();
                },
                None if self.check_inductive() => {return self.valid()}
                None => self.unfold()
            }
        }
    }
}


fn infer(_problem: Problem) -> VerificationResult {
    unimplemented!()
}