// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use anyhow::{Context as _, Result, bail};

const VARINT: u64 = 0;
const FIXED64: u64 = 1;
const LENGTH: u64 = 2;
const FIXED32: u64 = 5;

#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(crate) fn string(&mut self, field: u32, value: &str) {
        if value.is_empty() {
            return;
        }
        self.tag(field, LENGTH);
        self.varint(value.len() as u64);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub(crate) fn int32(&mut self, field: u32, value: i32) {
        if value == 0 {
            return;
        }
        self.tag(field, VARINT);
        self.varint(value as i64 as u64);
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn tag(&mut self, field: u32, kind: u64) {
        self.varint(u64::from(field) << 3 | kind);
    }

    fn varint(&mut self, mut value: u64) {
        while value >= 0x80 {
            self.bytes.push(value as u8 | 0x80);
            value >>= 7;
        }
        self.bytes.push(value as u8);
    }
}

pub(crate) enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub(crate) fn field(&mut self) -> Result<Option<(u32, Value<'a>)>> {
        if self.at >= self.bytes.len() {
            return Ok(None);
        }

        let key = self.varint()?;
        let value = match key & 7 {
            VARINT => Value::Varint(self.varint()?),
            LENGTH => Value::Bytes(self.slice()?),
            FIXED64 => self.skip(8)?,
            FIXED32 => self.skip(4)?,
            kind => bail!("unsupported wire type {kind}"),
        };

        Ok(Some(((key >> 3) as u32, value)))
    }

    fn varint(&mut self) -> Result<u64> {
        let mut value = 0;
        for shift in (0..64).step_by(7) {
            let byte = *self.bytes.get(self.at).context("truncated varint")?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        bail!("varint is too long")
    }

    fn slice(&mut self) -> Result<&'a [u8]> {
        let length = self.varint()? as usize;
        let end = self.at.checked_add(length).context("length overflows")?;
        let slice = self.bytes.get(self.at..end).context("truncated field")?;
        self.at = end;
        Ok(slice)
    }

    fn skip(&mut self, width: usize) -> Result<Value<'a>> {
        let end = self.at.checked_add(width).context("length overflows")?;
        if end > self.bytes.len() {
            bail!("truncated field");
        }
        self.at = end;
        Ok(Value::Fixed)
    }
}

pub(crate) fn text(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes)
        .context("field is not utf-8")?
        .to_owned())
}
