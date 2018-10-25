use super::{parser, serializer};
use nom::Offset;
use std::collections::VecDeque;
use mio::Ready;
use mio::unix::UnixReady;

#[derive(Clone,Debug,PartialEq)]
pub struct Frame {
  header: parser::FrameHeader,
  payload: Option<Vec<u8>>,
}

#[derive(Clone,Debug,PartialEq)]
pub enum St {
  Init,
  ClientPrefaceReceived,
  ServerPrefaceSent,
}

#[derive(Clone,Debug,PartialEq)]
pub struct State {
  pub front_output: VecDeque<Frame>,
  pub state: St,
  pub front_interest: UnixReady,
  //FIXME: make it configurable,
  pub max_frame_size: u32,
}

impl State {
  pub fn new() -> State {
    State {
      front_output: VecDeque::new(),
      state: St::Init,
      front_interest: UnixReady::from(Ready::readable()) | UnixReady::hup() | UnixReady::error(),
      max_frame_size: 16384,
    }
  }

  pub fn parse_front(&mut self, mut input: &[u8]) -> (usize, Result<parser::FrameHeader, ()>) {
    let mut consumed = 0usize;

    if self.state == St::Init {
      match parser::preface(input) {
        Err(e) => {
          error!("parser::preface error: {:?}", e);
          return (0, Err(()));
        },
        Ok((i, _)) => {
          consumed += input.offset(i);
          self.state = St::ClientPrefaceReceived;
          input = i;
        }
      }
    }


    match parser::frame(input, self.max_frame_size) {
      Err(e) => {
        error!("parser::frame error: {:?}", e);
        return (consumed, Err(()));
      },
      Ok((i, frame)) => {
        consumed += input.offset(i);
        (consumed, Ok(frame))
      }
    }
  }

  pub fn handle_front(&mut self, frame: &parser::FrameHeader) -> bool {
    match self.state {
      St::Init => true,
      St::ClientPrefaceReceived => {
        if frame.frame_type != parser::FrameType::Settings {
          panic!("invalid frame type");
        } else {
          let server_settings = Frame {
            header: parser::FrameHeader {
              payload_len: 0,
              frame_type: parser::FrameType::Settings,
              //FIXME: setting 1 for ACK?
              flags: 1,
              stream_id: 0,
            },
            payload: None,
          };

          self.front_output.push_back(server_settings);
          self.state = St::ServerPrefaceSent;
          self.front_interest.insert(UnixReady::from(Ready::writable()));
          true
        }
      },
      St::ServerPrefaceSent => {
        panic!("unknown frame for now: {:?}", frame);
      }
    }
  }


  pub fn gen_front(&mut self, mut output: &mut [u8]) -> Result<usize, ()> {
    if let Some(frame) = self.front_output.pop_front() {
      match serializer::gen_frame_header((output, 0), &frame.header) {
        Err(e) => {
          panic!("error serializing: {:?}", e);
        },
        Ok((sl, index)) => {
          Ok(index)
        }
      }
    } else {
      self.front_interest.remove(Ready::writable());
      Ok(0)
    }
  }
}
