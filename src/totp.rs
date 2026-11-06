//! TOTP (RFC 6238) from a pasted secret: a bare base32 key or an otpauth:// URI.

use hmac::{Hmac, Mac};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Clone, Debug)]
pub struct Secret {
    key: Vec<u8>,
    pub digits: u32,
    pub period: u64,
    pub algorithm: Algorithm,
}

impl Secret {
    /// Accepts `JBSW Y3DP-EHPK 3PXP`, lowercase, `=` padding, or a full
    /// `otpauth://totp/...?secret=...&digits=..&period=..&algorithm=..` URI.
    pub fn parse(input: &str) -> Result<Secret, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Empty".into());
        }
        if input.to_ascii_lowercase().starts_with("otpauth://") {
            return parse_uri(input);
        }
        Ok(Secret {
            key: decode_base32(input)?,
            digits: 6,
            period: 30,
            algorithm: Algorithm::Sha1,
        })
    }

    pub fn counter(&self, unix: u64) -> u64 {
        unix / self.period
    }

    pub fn seconds_left(&self, unix: u64) -> u64 {
        self.period - unix % self.period
    }

    pub fn code_at(&self, unix: u64) -> String {
        hotp(&self.key, self.counter(unix), self.digits, self.algorithm)
    }
}

fn parse_uri(uri: &str) -> Result<Secret, String> {
    let rest = &uri["otpauth://".len()..];
    if !rest.to_ascii_lowercase().starts_with("totp") {
        return Err("Only TOTP links are supported".into());
    }
    let query = rest.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut secret = None;
    let (mut digits, mut period, mut algorithm) = (6, 30, Algorithm::Sha1);
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode(v);
        match k.to_ascii_lowercase().as_str() {
            "secret" => secret = Some(v),
            "digits" => digits = v.parse().map_err(|_| "Bad digits")?,
            "period" => period = v.parse().map_err(|_| "Bad period")?,
            "algorithm" => {
                algorithm = match v.to_ascii_uppercase().as_str() {
                    "SHA1" => Algorithm::Sha1,
                    "SHA256" => Algorithm::Sha256,
                    "SHA512" => Algorithm::Sha512,
                    _ => return Err(format!("Unknown algorithm {v}")),
                }
            }
            _ => {}
        }
    }
    if !(6..=10).contains(&digits) || period == 0 {
        return Err("Bad digits or period".into());
    }
    let secret = secret.ok_or("Link has no secret")?;
    Ok(Secret { key: decode_base32(&secret)?, digits, period, algorithm })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Lenient RFC 4648 base32: ignores spaces, dashes and padding; case-insensitive.
fn decode_base32(s: &str) -> Result<Vec<u8>, String> {
    let mut bits: u64 = 0;
    let mut nbits = 0;
    let mut out = Vec::new();
    let mut chars = 0;
    for c in s.chars() {
        let v = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u64 - 'A' as u64,
            c @ '2'..='7' => c as u64 - '2' as u64 + 26,
            ' ' | '-' | '=' | '\t' | '\n' | '\r' => continue,
            _ => return Err("Not a base32 secret key".into()),
        };
        chars += 1;
        bits = (bits << 5) | v;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
            bits &= (1 << nbits) - 1;
        }
    }
    if chars < 16 {
        return Err("Too short for a secret key".into());
    }
    Ok(out)
}

fn hotp(key: &[u8], counter: u64, digits: u32, alg: Algorithm) -> String {
    let msg = counter.to_be_bytes();
    let hash = match alg {
        Algorithm::Sha1 => mac::<Hmac<sha1::Sha1>>(key, &msg),
        Algorithm::Sha256 => mac::<Hmac<sha2::Sha256>>(key, &msg),
        Algorithm::Sha512 => mac::<Hmac<sha2::Sha512>>(key, &msg),
    };
    let off = (hash[hash.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([hash[off] & 0x7f, hash[off + 1], hash[off + 2], hash[off + 3]]);
    let code = bin as u64 % 10u64.pow(digits);
    format!("{code:0width$}", width = digits as usize)
}

fn mac<M: Mac + hmac::digest::KeyInit>(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut m = <M as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(msg);
    m.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b32(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let (mut bits, mut n, mut s) = (0u64, 0, String::new());
        for &b in bytes {
            bits = (bits << 8) | b as u64;
            n += 8;
            while n >= 5 {
                n -= 5;
                s.push(A[((bits >> n) & 31) as usize] as char);
            }
        }
        if n > 0 {
            s.push(A[((bits << (5 - n)) & 31) as usize] as char);
        }
        s
    }

    // RFC 6238 Appendix B test vectors (8 digits).
    #[test]
    fn rfc6238_vectors() {
        let cases = [
            (Algorithm::Sha1, "12345678901234567890".to_string(), "SHA1"),
            (Algorithm::Sha256, "12345678901234567890123456789012".into(), "SHA256"),
            (
                Algorithm::Sha512,
                "1234567890123456789012345678901234567890123456789012345678901234".into(),
                "SHA512",
            ),
        ];
        let expected = [
            (59, ["94287082", "46119246", "90693936"]),
            (1111111109, ["07081804", "68084774", "25091201"]),
            (20000000000, ["65353130", "77737706", "47863826"]),
        ];
        for (i, (_, seed, name)) in cases.iter().enumerate() {
            let uri = format!("otpauth://totp/x?secret={}&digits=8&algorithm={name}", b32(seed.as_bytes()));
            let s = Secret::parse(&uri).unwrap();
            for (t, codes) in expected {
                assert_eq!(s.code_at(t), codes[i], "{name} at {t}");
            }
        }
    }

    #[test]
    fn lenient_bare_key() {
        let a = Secret::parse("  jbsw y3dp-ehpk 3pxp  ").unwrap();
        let b = Secret::parse("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(a.code_at(1_700_000_000), b.code_at(1_700_000_000));
        assert!(Secret::parse("hello world!").is_err());
        assert!(Secret::parse("ABCDEF").is_err());
    }
}
