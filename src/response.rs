//! Iron's HTTP Response representation and associated methods.

use std::io::{self, Read, Write};
use std::fmt::{self, Debug};

use typemap::TypeMap;
use plugin::Extensible;
use modifier::{Set, Modifier};

use hyper::header::Headers;

use status::{self, Status};
use {Plugin, headers};

pub use hyper::server::response::Response as HttpResponse;
use hyper::net::Fresh;

/// The response representation given to `Middleware`
pub struct Response {
    /// The response status-code.
    pub status: Option<Status>,

    /// The headers of the response.
    pub headers: Headers,

    /// A TypeMap to be used as an extensible storage for data
    /// associated with this Response.
    pub extensions: TypeMap,

    /// The body of the response.
    ///
    /// This is a Reader for generality, most data should
    /// be sent using either `serve` or `serve_file`.
    ///
    /// Arbitrary Readers can be sent by assigning to body.
    pub body: Option<Box<Read + Send>>
}

impl Response {
    /// Construct a blank Response
    pub fn new() -> Response {
        Response {
            status: None, // Start with no response code.
            body: None, // Start with no body.
            headers: Headers::new(),
            extensions: TypeMap::new()
        }
    }

    /// Construct a Response with the specified modifier pre-applied.
    pub fn with<M: Modifier<Response>>(m: M) -> Response {
        Response::new().set(m)
    }

    // `write_back` is used to put all the data added to `self`
    // back onto an `HttpResponse` so that it is sent back to the
    // client.
    //
    // `write_back` consumes the `Response`.
    #[doc(hidden)]
    pub fn write_back(self, mut http_res: HttpResponse<Fresh>) {
        *http_res.headers_mut() = self.headers;

        // Default to a 404 if no response code was set
        *http_res.status_mut() = self.status.clone().unwrap_or(status::NotFound);

        let out = match self.body {
            Some(body) => write_with_body(http_res, body),
            None => {
                http_res.headers_mut().set(headers::ContentLength(0));
                http_res.start().and_then(|res| res.end())
            }
        };

        match out {
            Err(e) => {
                error!("Error writing response: {}", e);
            },
            _ => {}
        }
    }
}

fn write_with_body(mut res: HttpResponse<Fresh>, mut body: Box<Read + Send>) -> io::Result<()> {
    let content_type = res.headers().get::<headers::ContentType>()
                           .map(|cx| cx.clone())
                           .unwrap_or_else(|| headers::ContentType("text/plain".parse().unwrap()));
    res.headers_mut().set(content_type);

    let mut res = try!(res.start());

    // FIXME: Manually inlined old_io::util::copy
    // because Box<Reader + Send> does not impl Reader.
    //
    // Tracking issue: rust-lang/rust#18542
    let mut buf = &mut [0; 1024 * 64];
    loop {
        let len = match body.read(buf) {
            Ok(0) => break,
            Ok(len) => len,
            Err(e) => { return Err(e) },
        };

        try!(res.write_all(&buf[..len]))
    }

    res.end()
}

impl Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "HTTP/1.1 {} {}\n{}",
            self.status.unwrap_or(status::NotFound),
            self.status.unwrap_or(status::NotFound).canonical_reason().unwrap(),
            self.headers
        )
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

// Allow plugins to attach to responses.
impl Extensible for Response {
    fn extensions(&self) -> &TypeMap {
        &self.extensions
    }

    fn extensions_mut(&mut self) -> &mut TypeMap {
        &mut self.extensions
    }
}

impl Plugin for Response {}
impl Set for Response {}

