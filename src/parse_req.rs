#[derive(Debug, PartialEq, Eq)]
pub enum BodyMode {
    None,
    ContentLength(usize),
    Chunked,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChunkState {
    Size,
    Data { remaining: usize },
    CRLF,
    Done,
}

#[derive(Debug)]
pub struct ParsedRequest {
	pub method: Option<String>,
    pub path: Option<String>,
    pub version: Option<u8>,
    pub headers: Option<Vec<(String, Vec<u8>)>>,
    pub body: Vec<u8>,
    pub body_mode: BodyMode,
}

// Determine the body_mode of the http request
pub fn determine_body_mode(req: &httparse::Request) -> BodyMode {
    for h in req.headers.iter() {
        if h.name.eq_ignore_ascii_case("Transfer-Encoding")
            && h.value.eq_ignore_ascii_case(b"chunked")
        {
            return BodyMode::Chunked;
        }
    }

    for h in req.headers.iter() {
        if h.name.eq_ignore_ascii_case("Content-Length") {
            if let Ok(len) = std::str::from_utf8(h.value).unwrap().parse::<usize>() {
                return BodyMode::ContentLength(len);
            }
        }
    }

    BodyMode::None
}

pub fn process_chunked(
    state: &mut ChunkState,
    read_buf: &mut Vec<u8>,
    body: &mut Vec<u8>,
) -> Result<bool, ()> {
    loop {
        match *state {
            ChunkState::Size => {
                if let Some(pos) = find_crlf(read_buf) {
                    let line = &read_buf[..pos];
                    let size_str = std::str::from_utf8(line).map_err(|_| ())?;
                    let size = usize::from_str_radix(size_str.trim(), 16).map_err(|_| ())?;

                    read_buf.drain(..pos + 2);

                    if size == 0 {
                        *state = ChunkState::CRLF;
                    } else {
                        *state = ChunkState::Data { remaining: size };
                    }
                } else {
                    return Ok(false);
                }
            }
            ChunkState::Data { ref mut remaining } => {
                if read_buf.is_empty() {
                    return Ok(false);
                }

                let to_take = (*remaining).min(read_buf.len());
                body.extend_from_slice(&read_buf[..to_take]);
                read_buf.drain(..to_take);
                *remaining -= to_take;

                if *remaining == 0 {
                    *state = ChunkState::CRLF;
                }
            }
            ChunkState::CRLF => {
                if read_buf.len() < 2 {
                    return Ok(false);
                }

                if &read_buf[..2] != b"\r\n" {
                    return Err(()); // protocole invalide
                }

                read_buf.drain(..2);
                *state = ChunkState::Size;
            }
            ChunkState::Done => {
                return Ok(true);
            }
        }
    }
}
fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}