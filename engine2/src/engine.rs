use super::options::{Config, EngineOpt};
use super::search::Search;
use super::session::Resp;
use super::time::TimingMode;
use super::uci::{Command, Error};

use core::position::Position;

use crossbeam_channel::{Receiver, Sender};

/// Manages the search and related configuration. This runs in a separate thread from the main
/// process.
pub struct Engine {
    /// Transmitter of messages to the Session thread.
    pub(super) tx: Sender<Resp>,
    /// Receiver of messages from the Session thread.
    pub(super) rx: Receiver<Command>,
    /// Current configuration of the engine.
    pub(super) config: Config,
    /// The internal board position.
    pub(super) pos: Option<Position>,
}

impl Engine {
    pub fn new(tx: Sender<Resp>, rx: Receiver<Command>) -> Self {
        // Since we are creating the engine, which includes a `Position`, we need to ensure that
        // the globals are initialised first. This is inexpensive if it has already been called
        // elsewhere.
        core::init::init_globals();

        Self {
            tx,
            rx,
            config: Default::default(),
            pos: Some(Default::default()),
        }
    }

    pub fn launch(&mut self) {
        loop {
            let s = self.rx.recv().unwrap();
            self.dispatch_command(s);
        }
    }

    fn dispatch_command(&mut self, cmd: Command) {
        match cmd {
            Command::Uci => self.command_uci(),
            Command::IsReady => self.command_isready(),
            Command::UciNewGame => self.command_ucinewgame(),
            Command::SetPosition((p, m)) => self.command_set_position(p, m),
            Command::SetOption(o) => self.command_set_option(o),
            Command::Go(tm) => self.command_go(tm),
            Command::Stop => todo!(),
            Command::Quit => todo!(),
            Command::Display => self.command_display(),
            Command::Config => self.command_config(),
        }
    }

    fn command_uci(&mut self) {
        self.report(Resp::Id);
        self.report(Resp::OptionsList);
        self.report(Resp::Uciok);
    }

    fn command_display(&self) {
        match &self.pos {
            Some(pos) => pos.pretty_print(),
            None => {}
        }
    }

    fn command_config(&self) {
        println!("{:#?}", self.config);
    }

    fn command_isready(&self) {
        self.tx.send(Resp::ReadyOk);
    }

    fn command_ucinewgame(&self) {}

    fn command_set_position(&mut self, pos: String, moves: Vec<String>) {
        match Position::from_fen(&pos) {
            Ok(mut pos) => {
                for mov in moves {
                    if pos.make_uci_move(&mov).is_none() {
                        self.tx.send(Resp::UciParseError(Error::InvalidMove));
                    }
                }

                self.pos = Some(pos)
            }
            Err(err) => self.report(Resp::UciParseError(Error::InvalidPosition(err))),
        }
    }

    fn command_set_option(&mut self, o: EngineOpt) {
        self.config.set_option(o);
    }

    fn command_go(&mut self, tm: TimingMode) {
        match self.pos.take() {
            Some(pos) => {
                let (_score, pos) = Search::new(pos).start_search(tm);
                self.pos = Some(pos);
            }
            None => unreachable!(
                "This method should never be called when a search is already in progress"
            ),
        }
    }

    fn report(&mut self, resp: Resp) {
        self.tx.send(resp);
    }
}