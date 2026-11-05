//! Open-Meteo (no key): current weather for `WEATHER_CITY`, cached 30 minutes. Becomes the
//! "за окном" line the Caster and the inhabitants see, so the room has a sky.

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::Serialize;

use super::http;

#[derive(Debug, Clone, Serialize)]
pub struct Weather {
    pub city: String,
    pub temp: f64,
    pub feels: f64,
    pub code: i64,
    pub text: String,
    pub wind: f64,
    pub is_day: bool,
    pub tmax: Option<f64>,
    pub tmin: Option<f64>,
    pub tomorrow: Option<String>,
    pub local_time: String,
    pub fetched_at: String,
}

struct Cache {
    city: String,
    at: Instant,
    w: Weather,
}

fn cache() -> &'static Mutex<Option<Cache>> {
    static C: OnceLock<Mutex<Option<Cache>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

pub fn describe(code: i64) -> &'static str {
    match code {
        0 => "ясно",
        1 => "в основном ясно",
        2 => "переменная облачность",
        3 => "пасмурно",
        45 | 48 => "туман",
        51 | 53 | 55 => "морось",
        56 | 57 => "ледяная морось",
        61 | 63 => "дождь",
        65 => "сильный дождь",
        66 | 67 => "ледяной дождь",
        71 | 73 => "снег",
        75 => "сильный снег",
        77 => "снежная крупа",
        80 | 81 => "ливень",
        82 => "сильный ливень",
        85 | 86 => "снегопад",
        95 => "гроза",
        96 | 99 => "гроза с градом",
        _ => "непонятно что",
    }
}

pub async fn current(city: &str) -> Result<Weather> {
    if let Some(c) = cache().lock().unwrap().as_ref() {
        if c.city == city && c.at.elapsed() < Duration::from_secs(30 * 60) {
            return Ok(c.w.clone());
        }
    }
    let geo: serde_json::Value = http()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", city), ("count", "1"), ("language", "ru"), ("format", "json")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let place = geo["results"].as_array().and_then(|a| a.first()).context("город не найден в Open-Meteo")?;
    let (lat, lon) = (place["latitude"].as_f64().unwrap_or(0.0), place["longitude"].as_f64().unwrap_or(0.0));
    let name = place["name"].as_str().unwrap_or(city).to_string();
    let v: serde_json::Value = http()
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("current", "temperature_2m,apparent_temperature,weather_code,wind_speed_10m,is_day".into()),
            ("daily", "temperature_2m_max,temperature_2m_min,weather_code".into()),
            ("timezone", "auto".into()),
            ("forecast_days", "2".into()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let cur = &v["current"];
    let code = cur["weather_code"].as_i64().unwrap_or(-1);
    let daily = &v["daily"];
    let w = Weather {
        city: name,
        temp: cur["temperature_2m"].as_f64().unwrap_or(0.0),
        feels: cur["apparent_temperature"].as_f64().unwrap_or(0.0),
        code,
        text: describe(code).to_string(),
        wind: cur["wind_speed_10m"].as_f64().unwrap_or(0.0),
        is_day: cur["is_day"].as_i64().unwrap_or(1) == 1,
        tmax: daily["temperature_2m_max"].as_array().and_then(|a| a.first()).and_then(|x| x.as_f64()),
        tmin: daily["temperature_2m_min"].as_array().and_then(|a| a.first()).and_then(|x| x.as_f64()),
        tomorrow: daily["weather_code"].as_array().and_then(|a| a.get(1)).and_then(|x| x.as_i64()).map(|c| {
            let hi = daily["temperature_2m_max"].as_array().and_then(|a| a.get(1)).and_then(|x| x.as_f64()).unwrap_or(0.0);
            format!("{} до {hi:+.0}°", describe(c))
        }),
        local_time: cur["time"].as_str().unwrap_or("").to_string(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
    };
    *cache().lock().unwrap() = Some(Cache { city: city.to_string(), at: Instant::now(), w: w.clone() });
    Ok(w)
}

/// One line for prompts: "за окном (Москва, 17:40, вечер): +3°, дождь, ветер 6 м/с; завтра пасмурно до +5°".
pub fn line(w: &Weather) -> String {
    let hour: u32 = w.local_time.get(11..13).and_then(|h| h.parse().ok()).unwrap_or(12);
    let part = match hour {
        5..=10 => "утро",
        11..=16 => "день",
        17..=22 => "вечер",
        _ => "ночь",
    };
    format!(
        "за окном ({}, {}, {part}): {:+.0}°{}, {}, ветер {:.0} м/с{}",
        w.city,
        w.local_time.get(11..16).unwrap_or("?"),
        w.temp,
        if (w.feels - w.temp).abs() >= 3.0 { format!(" (ощущается {:+.0}°)", w.feels) } else { String::new() },
        w.text,
        w.wind / 3.6,
        w.tomorrow.as_ref().map(|t| format!("; завтра {t}")).unwrap_or_default()
    )
}
