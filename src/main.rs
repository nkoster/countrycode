use std::{
    io::{self, BufRead, Write},
    net::Ipv4Addr,
    time::Duration,
};

use regex::bytes::Regex;
use reqwest::blocking::Client;
use serde_json::Value;

fn country_code(client: &Client, ip: &str) -> String {
    let Ok(ipv4) = ip.parse::<Ipv4Addr>() else {
        return "??".to_string();
    };

    // RFC 1918 private IPv4 ranges are local LAN addresses and need no lookup.
    if should_skip_lookup(ipv4) {
        return String::new();
    }

    let url = format!("https://geoip.w3b.net/{ip}");

    let result = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json::<Value>());

    match result {
        Ok(json) => json["countryCode"]
            .as_str()
            .filter(|s| s.len() == 2)
            .unwrap_or("??")
            .to_uppercase(),
        Err(_) => "??".to_string(),
    }
}

fn should_skip_lookup(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 127
        || octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

fn first_ipv4_match<'a>(re: &Regex, line: &'a [u8]) -> Option<regex::bytes::Match<'a>> {
    re.find_iter(line).find(|m| {
        let before = line.get(..m.start()).and_then(|s| s.last()).copied();
        let after = line.get(m.end()).copied();
        let boundary_ok = !before.is_some_and(|b| b.is_ascii_digit() || b == b'.')
            && !after.is_some_and(|b| b.is_ascii_digit() || b == b'.');

        let valid_ip_and_port = std::str::from_utf8(m.as_bytes())
            .ok()
            .and_then(|s| {
                let mut parts = s.split('.');
                let ip = [parts.next()?, parts.next()?, parts.next()?, parts.next()?].join(".");
                let port = parts.next();
                if parts.next().is_some() || port.is_some_and(|p| p.parse::<u16>().is_err()) {
                    return None;
                }
                ip.parse::<Ipv4Addr>().ok()
            })
            .is_some();

        boundary_ok && valid_ip_and_port
    })
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    let mut input = stdin.lock();
    let mut output = stdout.lock();

    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("HTTP client initialization failed");

    // tcpdump commonly prints IPv4 endpoints as `address.port`, for example
    // `8.8.8.8.53`. Capture the optional port with the address so the trailing
    // `.53` is not mistaken for another IPv4 octet.
    let re = Regex::new(r"(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?:\.[0-9]{1,5})?").unwrap();

    let mut line = Vec::new();

    loop {
        line.clear();

        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }

        let found = first_ipv4_match(&re, &line);

        if let Some(m) = found {
            let value = std::str::from_utf8(m.as_bytes()).unwrap();
            // Keep all four IP octets together; a fifth component is the port.
            let (ip, suffix) = match value.match_indices('.').nth(3) {
                Some((split, _)) => (&value[..split], &value[split..]),
                None => (value, ""),
            };
            let country = country_code(&client, ip);

            output.write_all(&line[..m.start()])?;
            if !country.is_empty() {
                write!(output, "{country}.")?;
            }
            output.write_all(ip.as_bytes())?;
            output.write_all(suffix.as_bytes())?;
            output.write_all(&line[m.end()..])?;
        } else {
            output.write_all(&line)?;
        }

        output.flush()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{country_code, first_ipv4_match, should_skip_lookup};
    use regex::bytes::Regex;

    fn find(line: &str) -> Option<String> {
        let re = Regex::new(r"(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?:\.[0-9]{1,5})?").unwrap();
        first_ipv4_match(&re, line.as_bytes())
            .map(|m| String::from_utf8(m.as_bytes().to_vec()).unwrap())
    }

    #[test]
    fn recognizes_tcpdump_ip_and_port() {
        assert_eq!(
            find("IP 8.8.8.8.53 > 1.1.1.1.53"),
            Some("8.8.8.8.53".into())
        );
    }

    #[test]
    fn recognizes_plain_ipv4() {
        assert_eq!(find("host 192.0.2.1:443"), Some("192.0.2.1".into()));
    }

    #[test]
    fn keeps_only_the_first_match_on_a_line() {
        assert_eq!(find("8.8.8.8.53 > 1.1.1.1.53"), Some("8.8.8.8.53".into()));
    }

    #[test]
    fn rejects_invalid_ipv4_and_ports() {
        assert_eq!(find("999.8.8.8.53"), None);
        assert_eq!(find("8.8.8.8.70000"), None);
    }

    #[test]
    fn recognizes_private_and_loopback_ranges_and_boundaries() {
        for ip in ["10.0.0.1", "172.16.0.1", "172.31.255.254", "192.168.1.1"] {
            assert!(
                should_skip_lookup(ip.parse().unwrap()),
                "{ip} should skip lookup"
            );
        }
        for ip in ["127.0.0.1", "127.255.255.254"] {
            assert!(
                should_skip_lookup(ip.parse().unwrap()),
                "{ip} should skip lookup"
            );
        }
        for ip in [
            "9.255.255.255",
            "172.15.255.255",
            "172.32.0.0",
            "192.167.1.1",
            "126.255.255.255",
            "128.0.0.0",
        ] {
            assert!(
                !should_skip_lookup(ip.parse().unwrap()),
                "{ip} should use lookup"
            );
        }
    }

    #[test]
    fn private_address_is_left_unprefixed_without_geoip_lookup() {
        let client = reqwest::blocking::Client::new();
        assert_eq!(country_code(&client, "192.168.1.10"), "");
        assert_eq!(country_code(&client, "127.0.0.1"), "");
    }
}
