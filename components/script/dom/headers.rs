/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::HeadersBinding::{
    HeadersInit, HeadersMethods, HeadersWrap,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::iterable::Iterable;
use crate::dom::bindings::reflector::{reflect_dom_object, Reflector};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::{is_token, ByteString};
use crate::dom::globalscope::GlobalScope;
use dom_struct::dom_struct;
use http::header::{self, HeaderMap as HyperHeaders, HeaderName, HeaderValue};
use mime::{self, Mime};
use std::cell::Cell;
use std::result::Result;
use std::str::{self, FromStr};

#[dom_struct]
pub struct Headers {
    reflector_: Reflector,
    guard: Cell<Guard>,
    #[ignore_malloc_size_of = "Defined in hyper"]
    header_list: DomRefCell<HyperHeaders>,
}

// https://fetch.spec.whatwg.org/#concept-headers-guard
#[derive(Clone, Copy, JSTraceable, MallocSizeOf, PartialEq)]
pub enum Guard {
    Immutable,
    Request,
    RequestNoCors,
    Response,
    None,
}

impl Headers {
    pub fn new_inherited() -> Headers {
        Headers {
            reflector_: Reflector::new(),
            guard: Cell::new(Guard::None),
            header_list: DomRefCell::new(HyperHeaders::new()),
        }
    }

    pub fn new(global: &GlobalScope) -> DomRoot<Headers> {
        reflect_dom_object(Box::new(Headers::new_inherited()), global, HeadersWrap)
    }

    // https://fetch.spec.whatwg.org/#dom-headers
    pub fn Constructor(
        global: &GlobalScope,
        init: Option<HeadersInit>,
    ) -> Fallible<DomRoot<Headers>> {
        let dom_headers_new = Headers::new(global);
        dom_headers_new.fill(init)?;
        Ok(dom_headers_new)
    }
}

impl HeadersMethods for Headers {
    // https://fetch.spec.whatwg.org/#concept-headers-append
    fn Append(&self, name: ByteString, value: ByteString) -> ErrorResult {
        // Step 1
        let value = normalize_value(value);
        // Step 2
        let (mut valid_name, valid_value) = validate_name_and_value(name, value)?;
        valid_name = valid_name.to_lowercase();
        // Step 3
        if self.guard.get() == Guard::Immutable {
            return Err(Error::Type("Guard is immutable".to_string()));
        }
        // Step 4
        if self.guard.get() == Guard::Request && is_forbidden_header_name(&valid_name) {
            return Ok(());
        }
        // Step 5
        if self.guard.get() == Guard::RequestNoCors &&
            !is_cors_safelisted_request_header(&valid_name, &valid_value)
        {
            return Ok(());
        }
        // Step 6
        if self.guard.get() == Guard::Response && is_forbidden_response_header(&valid_name) {
            return Ok(());
        }
        // Step 7
        let mut combined_value: Vec<u8> = vec![];
        if let Some(v) = self
            .header_list
            .borrow()
            .get(HeaderName::from_str(&valid_name).unwrap())
        {
            combined_value = v.as_bytes().to_vec();
            combined_value.push(b',');
        }
        combined_value.extend(valid_value.iter().cloned());
        self.header_list.borrow_mut().insert(
            HeaderName::from_str(&valid_name).unwrap(),
            HeaderValue::from_bytes(&combined_value).unwrap(),
        );
        Ok(())
    }

    // https://fetch.spec.whatwg.org/#dom-headers-delete
    fn Delete(&self, name: ByteString) -> ErrorResult {
        // Step 1
        let valid_name = validate_name(name)?;
        // Step 2
        if self.guard.get() == Guard::Immutable {
            return Err(Error::Type("Guard is immutable".to_string()));
        }
        // Step 3
        if self.guard.get() == Guard::Request && is_forbidden_header_name(&valid_name) {
            return Ok(());
        }
        // Step 4
        if self.guard.get() == Guard::RequestNoCors &&
            !is_cors_safelisted_request_header(&valid_name, &b"invalid".to_vec())
        {
            return Ok(());
        }
        // Step 5
        if self.guard.get() == Guard::Response && is_forbidden_response_header(&valid_name) {
            return Ok(());
        }
        // Step 6
        self.header_list.borrow_mut().remove(&valid_name);
        Ok(())
    }

    // https://fetch.spec.whatwg.org/#dom-headers-get
    fn Get(&self, name: ByteString) -> Fallible<Option<ByteString>> {
        // Step 1
        let valid_name = validate_name(name)?;
        Ok(self
            .header_list
            .borrow()
            .get(HeaderName::from_str(&valid_name).unwrap())
            .map(|v| ByteString::new(v.as_bytes().to_vec())))
    }

    // https://fetch.spec.whatwg.org/#dom-headers-has
    fn Has(&self, name: ByteString) -> Fallible<bool> {
        // Step 1
        let valid_name = validate_name(name)?;
        // Step 2
        Ok(self.header_list.borrow_mut().get(&valid_name).is_some())
    }

    // https://fetch.spec.whatwg.org/#dom-headers-set
    fn Set(&self, name: ByteString, value: ByteString) -> Fallible<()> {
        // Step 1
        let value = normalize_value(value);
        // Step 2
        let (mut valid_name, valid_value) = validate_name_and_value(name, value)?;
        valid_name = valid_name.to_lowercase();
        // Step 3
        if self.guard.get() == Guard::Immutable {
            return Err(Error::Type("Guard is immutable".to_string()));
        }
        // Step 4
        if self.guard.get() == Guard::Request && is_forbidden_header_name(&valid_name) {
            return Ok(());
        }
        // Step 5
        if self.guard.get() == Guard::RequestNoCors &&
            !is_cors_safelisted_request_header(&valid_name, &valid_value)
        {
            return Ok(());
        }
        // Step 6
        if self.guard.get() == Guard::Response && is_forbidden_response_header(&valid_name) {
            return Ok(());
        }
        // Step 7
        // https://fetch.spec.whatwg.org/#concept-header-list-set
        self.header_list.borrow_mut().insert(
            HeaderName::from_str(&valid_name).unwrap(),
            HeaderValue::from_bytes(&valid_value).unwrap(),
        );
        Ok(())
    }
}

impl Headers {
    // https://fetch.spec.whatwg.org/#concept-headers-fill
    pub fn fill(&self, filler: Option<HeadersInit>) -> ErrorResult {
        match filler {
            // Step 1
            Some(HeadersInit::Headers(h)) => {
                for (name, value) in h.header_list.borrow().iter() {
                    self.Append(
                        ByteString::new(Vec::from(name.as_str())),
                        ByteString::new(Vec::from(value.to_str().unwrap().as_bytes())),
                    )?;
                }
                Ok(())
            },
            // Step 2
            Some(HeadersInit::ByteStringSequenceSequence(v)) => {
                for mut seq in v {
                    if seq.len() == 2 {
                        let val = seq.pop().unwrap();
                        let name = seq.pop().unwrap();
                        self.Append(name, val)?;
                    } else {
                        return Err(Error::Type(
                            format!("Each header object must be a sequence of length 2 - found one with length {}",
                                    seq.len())));
                    }
                }
                Ok(())
            },
            Some(HeadersInit::StringByteStringRecord(m)) => {
                for (key, value) in m.iter() {
                    let key_vec = key.as_ref().to_string().into();
                    let headers_key = ByteString::new(key_vec);
                    self.Append(headers_key, value.clone())?;
                }
                Ok(())
            },
            None => Ok(()),
        }
    }

    pub fn for_request(global: &GlobalScope) -> DomRoot<Headers> {
        let headers_for_request = Headers::new(global);
        headers_for_request.guard.set(Guard::Request);
        headers_for_request
    }

    pub fn for_response(global: &GlobalScope) -> DomRoot<Headers> {
        let headers_for_response = Headers::new(global);
        headers_for_response.guard.set(Guard::Response);
        headers_for_response
    }

    pub fn set_guard(&self, new_guard: Guard) {
        self.guard.set(new_guard)
    }

    pub fn get_guard(&self) -> Guard {
        self.guard.get()
    }

    pub fn empty_header_list(&self) {
        *self.header_list.borrow_mut() = HyperHeaders::new();
    }

    pub fn set_headers(&self, hyper_headers: HyperHeaders) {
        *self.header_list.borrow_mut() = hyper_headers;
    }

    pub fn get_headers_list(&self) -> HyperHeaders {
        self.header_list.borrow_mut().clone()
    }

    // https://fetch.spec.whatwg.org/#concept-header-extract-mime-type
    pub fn extract_mime_type(&self) -> Vec<u8> {
        self.header_list
            .borrow()
            .get(header::CONTENT_TYPE)
            .map_or(vec![], |v| v.as_bytes().to_owned())
    }

    pub fn sort_header_list(&self) -> Vec<(String, String)> {
        let borrowed_header_list = self.header_list.borrow();
        let headers_iter = borrowed_header_list.iter();
        let mut header_vec = vec![];
        for (name, value) in headers_iter {
            let name = name.as_str().to_owned();
            let value = value.to_str().unwrap().to_owned();
            let name_value = (name, value);
            header_vec.push(name_value);
        }
        header_vec.sort();
        header_vec
    }
}

impl Iterable for Headers {
    type Key = ByteString;
    type Value = ByteString;

    fn get_iterable_length(&self) -> u32 {
        self.header_list.borrow().iter().count() as u32
    }

    fn get_value_at_index(&self, n: u32) -> ByteString {
        let sorted_header_vec = self.sort_header_list();
        let value = sorted_header_vec[n as usize].1.clone();
        ByteString::new(value.into_bytes().to_vec())
    }

    fn get_key_at_index(&self, n: u32) -> ByteString {
        let sorted_header_vec = self.sort_header_list();
        let key = sorted_header_vec[n as usize].0.clone();
        ByteString::new(key.into_bytes().to_vec())
    }
}

fn is_cors_safelisted_request_content_type(value: &[u8]) -> bool {
    let value_string = if let Ok(s) = str::from_utf8(value) {
        s
    } else {
        return false;
    };
    let value_mime_result: Result<Mime, _> = value_string.parse();
    match value_mime_result {
        Err(_) => false,
        Ok(value_mime) => match (value_mime.type_(), value_mime.subtype()) {
            (mime::APPLICATION, mime::WWW_FORM_URLENCODED) |
            (mime::MULTIPART, mime::FORM_DATA) |
            (mime::TEXT, mime::PLAIN) => true,
            _ => false,
        },
    }
}

// TODO: "DPR", "Downlink", "Save-Data", "Viewport-Width", "Width":
// ... once parsed, the value should not be failure.
// https://fetch.spec.whatwg.org/#cors-safelisted-request-header
fn is_cors_safelisted_request_header(name: &str, value: &[u8]) -> bool {
    match name {
        "accept" | "accept-language" | "content-language" => true,
        "content-type" => is_cors_safelisted_request_content_type(value),
        _ => false,
    }
}

// https://fetch.spec.whatwg.org/#forbidden-response-header-name
fn is_forbidden_response_header(name: &str) -> bool {
    match name {
        "set-cookie" | "set-cookie2" => true,
        _ => false,
    }
}

// https://fetch.spec.whatwg.org/#forbidden-header-name
pub fn is_forbidden_header_name(name: &str) -> bool {
    let disallowed_headers = [
        "accept-charset",
        "accept-encoding",
        "access-control-request-headers",
        "access-control-request-method",
        "connection",
        "content-length",
        "cookie",
        "cookie2",
        "date",
        "dnt",
        "expect",
        "host",
        "keep-alive",
        "origin",
        "referer",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "via",
    ];

    let disallowed_header_prefixes = ["sec-", "proxy-"];

    disallowed_headers.iter().any(|header| *header == name) || disallowed_header_prefixes
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

// There is some unresolved confusion over the definition of a name and a value.
// The fetch spec [1] defines a name as "a case-insensitive byte
// sequence that matches the field-name token production. The token
// productions are viewable in [2]." A field-name is defined as a
// token, which is defined in [3].
// ISSUE 1:
// It defines a value as "a byte sequence that matches the field-content token production."
// To note, there is a difference between field-content and
// field-value (which is made up of field-content and obs-fold). The
// current definition does not allow for obs-fold (which are white
// space and newlines) in values. So perhaps a value should be defined
// as "a byte sequence that matches the field-value token production."
// However, this would then allow values made up entirely of white space and newlines.
// RELATED ISSUE 2:
// According to a previously filed Errata ID: 4189 in [4], "the
// specified field-value rule does not allow single field-vchar
// surrounded by whitespace anywhere". They provided a fix for the
// field-content production, but ISSUE 1 has still not been resolved.
// The production definitions likely need to be re-written.
// [1] https://fetch.spec.whatwg.org/#concept-header-value
// [2] https://tools.ietf.org/html/rfc7230#section-3.2
// [3] https://tools.ietf.org/html/rfc7230#section-3.2.6
// [4] https://www.rfc-editor.org/errata_search.php?rfc=7230
fn validate_name_and_value(name: ByteString, value: ByteString) -> Fallible<(String, Vec<u8>)> {
    let valid_name = validate_name(name)?;
    if !is_field_content(&value) {
        return Err(Error::Type("Value is not valid".to_string()));
    }
    Ok((valid_name, value.into()))
}

fn validate_name(name: ByteString) -> Fallible<String> {
    if !is_field_name(&name) {
        return Err(Error::Type("Name is not valid".to_string()));
    }
    match String::from_utf8(name.into()) {
        Ok(ns) => Ok(ns),
        _ => Err(Error::Type("Non-UTF8 header name found".to_string())),
    }
}

// Removes trailing and leading HTTP whitespace bytes.
// https://fetch.spec.whatwg.org/#concept-header-value-normalize
pub fn normalize_value(value: ByteString) -> ByteString {
    match (
        index_of_first_non_whitespace(&value),
        index_of_last_non_whitespace(&value),
    ) {
        (Some(begin), Some(end)) => ByteString::new(value[begin..end + 1].to_owned()),
        _ => ByteString::new(vec![]),
    }
}

fn is_HTTP_whitespace(byte: u8) -> bool {
    byte == b'\t' || byte == b'\n' || byte == b'\r' || byte == b' '
}

fn index_of_first_non_whitespace(value: &ByteString) -> Option<usize> {
    for (index, &byte) in value.iter().enumerate() {
        if !is_HTTP_whitespace(byte) {
            return Some(index);
        }
    }
    None
}

fn index_of_last_non_whitespace(value: &ByteString) -> Option<usize> {
    for (index, &byte) in value.iter().enumerate().rev() {
        if !is_HTTP_whitespace(byte) {
            return Some(index);
        }
    }
    None
}

// http://tools.ietf.org/html/rfc7230#section-3.2
fn is_field_name(name: &ByteString) -> bool {
    is_token(&*name)
}

// https://tools.ietf.org/html/rfc7230#section-3.2
// http://www.rfc-editor.org/errata_search.php?rfc=7230
// Errata ID: 4189
// field-content = field-vchar [ 1*( SP / HTAB / field-vchar )
//                               field-vchar ]
fn is_field_content(value: &ByteString) -> bool {
    let value_len = value.len();

    if value_len == 0 {
        return false;
    }
    if !is_field_vchar(value[0]) {
        return false;
    }

    if value_len > 2 {
        for &ch in &value[1..value_len - 1] {
            if !is_field_vchar(ch) && !is_space(ch) && !is_htab(ch) {
                return false;
            }
        }
    }

    if !is_field_vchar(value[value_len - 1]) {
        return false;
    }

    return true;
}

fn is_space(x: u8) -> bool {
    x == b' '
}

fn is_htab(x: u8) -> bool {
    x == b'\t'
}

// https://tools.ietf.org/html/rfc7230#section-3.2
fn is_field_vchar(x: u8) -> bool {
    is_vchar(x) || is_obs_text(x)
}

// https://tools.ietf.org/html/rfc5234#appendix-B.1
pub fn is_vchar(x: u8) -> bool {
    match x {
        0x21...0x7E => true,
        _ => false,
    }
}

// http://tools.ietf.org/html/rfc7230#section-3.2.6
pub fn is_obs_text(x: u8) -> bool {
    match x {
        0x80...0xFF => true,
        _ => false,
    }
}
