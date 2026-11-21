use super::{Result, Secret};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Totp {
    seed: Secret,
    algorithm: Algorithm,
    digits: u32,
    period: u64,
}
impl Totp {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        let (seed, algorithm, digits, period) = if value.trim().starts_with("otpauth://") {
            let uri = url::Url::parse(value.trim())
                .map_err(|_| "Invalid authenticator URI.".to_owned())?;
            if uri.host_str() != Some("totp") {
                return Err("Only time-based authenticator codes are supported.".into());
            }
            let mut seed = None;
            let mut algorithm = Algorithm::Sha1;
            let mut digits = 6;
            let mut period = 30;
            let mut seen = std::collections::BTreeSet::new();
            for (key, value) in uri.query_pairs() {
                if !seen.insert(key.to_string()) {
                    return Err("The authenticator URI repeats an option.".into());
                }
                match key.as_ref() {
                    "secret" => seed = Some(value.into_owned()),
                    "algorithm" => {
                        algorithm = match value.to_ascii_uppercase().as_str() {
                            "SHA1" => Algorithm::Sha1,
                            "SHA256" => Algorithm::Sha256,
                            "SHA512" => Algorithm::Sha512,
                            _ => {
                                return Err(
                                    "Use SHA1, SHA256 or SHA512 for authenticator codes.".into()
                                );
                            }
                        }
                    }
                    "digits" => {
                        digits = value
                            .parse()
                            .map_err(|_| "Invalid authenticator code length.".to_owned())?;
                    }
                    "period" => {
                        period = value
                            .parse()
                            .map_err(|_| "Invalid authenticator period.".to_owned())?;
                    }
                    // The label's issuer, an icon and whatever else an
                    // exporter adds do not change the codes.
                    _ => {}
                }
            }
            (
                seed.ok_or("The authenticator URI has no secret.")?,
                algorithm,
                digits,
                period,
            )
        } else {
            (value.trim().to_owned(), Algorithm::Sha1, 6, 30)
        };
        let seed = Zeroizing::new(seed);
        let normalized = seed
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && !matches!(c, '=' | '-'))
            .flat_map(char::to_uppercase)
            .collect();
        let totp = Self {
            seed: Secret::new(normalized),
            algorithm,
            digits,
            period,
        };
        totp.validate()?;
        Ok(totp)
    }
    fn key(&self) -> Result<Zeroizing<Vec<u8>>> {
        if self.seed.expose().len() > 2048 {
            return Err("The authenticator secret is too long.".into());
        }
        data_encoding::BASE32_NOPAD
            .decode(self.seed.expose().as_bytes())
            .map(Zeroizing::new)
            .map_err(|_| "Enter a valid Base32 authenticator secret.".into())
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if !(6..=8).contains(&self.digits)
            || !(1..=3600).contains(&self.period)
            || self.key()?.len() < 10
        {
            return Err(
                "Use a valid authenticator secret, 6–8 digits and a period of 1–3600 seconds."
                    .into(),
            );
        }
        Ok(())
    }
    pub(crate) fn code(&self, seconds: u64) -> Result<(Secret, u64)> {
        self.validate()?;
        let key = self.key()?;
        let counter = (seconds / self.period).to_be_bytes();
        let digest = Zeroizing::new(
            match self.algorithm {
                Algorithm::Sha1 => Hmac::<sha1::Sha1>::new_from_slice(&key).map(|mut mac| {
                    mac.update(&counter);
                    mac.finalize().into_bytes().to_vec()
                }),
                Algorithm::Sha256 => Hmac::<sha2::Sha256>::new_from_slice(&key).map(|mut mac| {
                    mac.update(&counter);
                    mac.finalize().into_bytes().to_vec()
                }),
                Algorithm::Sha512 => Hmac::<sha2::Sha512>::new_from_slice(&key).map(|mut mac| {
                    mac.update(&counter);
                    mac.finalize().into_bytes().to_vec()
                }),
            }
            .map_err(|_| "The authenticator code could not be generated.".to_owned())?,
        );
        let offset = usize::from(digest[digest.len() - 1] & 15);
        let value = u32::from_be_bytes(
            digest[offset..offset + 4]
                .try_into()
                .map_err(|_| "Invalid authenticator output.".to_owned())?,
        ) & 0x7fff_ffff;
        let code = format!(
            "{:0width$}",
            value % 10u32.pow(self.digits),
            width = self.digits as usize
        );
        Ok((Secret::new(code), self.period - seconds % self.period))
    }
    pub(crate) fn uri_secret(&self) -> Secret {
        Secret::new(format!(
            "otpauth://totp/enzim?secret={}&algorithm={}&digits={}&period={}",
            self.seed.expose(),
            match self.algorithm {
                Algorithm::Sha1 => "SHA1",
                Algorithm::Sha256 => "SHA256",
                Algorithm::Sha512 => "SHA512",
            },
            self.digits,
            self.period
        ))
    }
}
