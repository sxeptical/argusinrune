//! Data.gov.sg weather aggregation (2-hour forecast, PSI, temperature,
//! humidity, UV, 1-hour PM2.5, four-day outlook). Ports
//! `lib/api-clients/weather.ts`: individual feeds fail independently and only
//! all of them failing is an error. Partial snapshots get a 30s cache TTL.

use std::time::Duration;

use argus_core::{
    air_quality::{get_pm25_band, get_psi_status},
    api_types::{
        DataGovForecastResponse, DataGovFourDayResponse, DataGovPsiResponse,
        DataGovStationReadingsResponse, DataGovUvResponse, RegionalReading,
    },
    types::{RegionalAirReadings, UvStatus, WeatherData, WeatherForecastDay},
};
use serde::de::DeserializeOwned;

use crate::{
    Client, Result,
    error::Error,
    health::Source,
    http::DEFAULT_TIMEOUT_MS,
    time::{latest_iso_timestamp, parse_timestamp},
};

const DATA_GOV_BASE_URL: &str = "https://api.data.gov.sg/v1/environment";
const DATA_GOV_V2_BASE_URL: &str = "https://api-open.data.gov.sg/v2/real-time/api";
const WEATHER_TTL: Duration = Duration::from_secs(5 * 60);
const WEATHER_PARTIAL_TTL: Duration = Duration::from_secs(30);
const AIR_REGIONS: &[fn(&RegionalAirReadings) -> Option<f64>] = &[
    |r| r.north,
    |r| r.south,
    |r| r.east,
    |r| r.west,
    |r| r.central,
];

pub fn get_uv_status(uv: Option<f64>) -> UvStatus {
    let Some(uv) = uv.filter(|v| v.is_finite()) else {
        return UvStatus::Unknown;
    };
    if uv <= 2.0 {
        UvStatus::Low
    } else if uv <= 5.0 {
        UvStatus::Moderate
    } else if uv <= 7.0 {
        UvStatus::High
    } else if uv <= 10.0 {
        UvStatus::VeryHigh
    } else {
        UvStatus::Extreme
    }
}

fn latest_record<T>(records: &[T], timestamp: impl Fn(&T) -> Option<&str>) -> Option<&T> {
    records
        .iter()
        .max_by_key(|r| timestamp(r).and_then(parse_timestamp).unwrap_or(i64::MIN))
}

pub fn extract_latest_uv(response: &DataGovUvResponse) -> Option<f64> {
    let record = latest_record(&response.data.records, |r| Some(&r.timestamp))?;
    record
        .index
        .iter()
        .filter(|e| e.value.is_finite())
        .max_by_key(|e| parse_timestamp(&e.hour).unwrap_or(i64::MIN))
        .map(|e| e.value)
}

fn average(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let finite: Vec<f64> = values.into_iter().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    let mean = finite.iter().sum::<f64>() / finite.len() as f64;
    Some((mean * 10.0).round() / 10.0)
}

fn regional(reading: Option<&RegionalReading>) -> RegionalAirReadings {
    let get =
        |f: fn(&RegionalReading) -> Option<f64>| reading.and_then(f).filter(|v| v.is_finite());
    RegionalAirReadings {
        north: get(|r| r.north),
        south: get(|r| r.south),
        east: get(|r| r.east),
        west: get(|r| r.west),
        central: get(|r| r.central),
    }
}

fn worst_or_national(regions: &RegionalAirReadings, raw: Option<&RegionalReading>) -> Option<f64> {
    let worst = AIR_REGIONS
        .iter()
        .filter_map(|f| f(regions))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    worst.or_else(|| raw.and_then(|r| r.national).filter(|v| v.is_finite()))
}

/// Assemble a `WeatherData` snapshot from feeds that may each be missing.
/// Returns `(data, partial)`.
#[allow(clippy::too_many_arguments)]
pub fn assemble_weather(
    forecast: Option<&DataGovForecastResponse>,
    psi: Option<&DataGovPsiResponse>,
    temperature: Option<&DataGovStationReadingsResponse>,
    humidity: Option<&DataGovStationReadingsResponse>,
    uv: Option<&DataGovUvResponse>,
    pm25: Option<&argus_core::api_types::DataGovPm25Response>,
    four_day: Option<&DataGovFourDayResponse>,
) -> (WeatherData, bool) {
    let partial = [
        forecast.is_some(),
        psi.is_some(),
        temperature.is_some(),
        humidity.is_some(),
        uv.is_some(),
        pm25.is_some(),
        four_day.is_some(),
    ]
    .iter()
    .any(|ok| !ok);

    // The forecast has no national field, so use the most common area value.
    let forecasts: Vec<&argus_core::api_types::AreaForecast> = forecast
        .and_then(|f| f.items.as_ref())
        .and_then(|items| items.first())
        .and_then(|item| item.forecasts.as_ref())
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    let forecast_text = if forecasts.is_empty() {
        "No forecast available".to_string()
    } else {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for entry in &forecasts {
            *counts.entry(entry.forecast.as_str()).or_default() += 1;
        }
        let majority = forecasts[0].forecast.as_str();
        let mut best = majority;
        let mut best_count = 0;
        for (text, count) in counts {
            if count > best_count {
                best_count = count;
                best = text;
            }
        }
        format!("Island-wide: {best}")
    };

    let psi_item = psi.and_then(|p| p.items.as_ref()).and_then(|i| i.first());
    let psi_raw = psi_item
        .and_then(|i| i.readings.as_ref())
        .and_then(|r| r.psi_twenty_four_hourly.as_ref());
    let pm25_item = pm25.and_then(|p| p.data.items.first());
    let pm25_raw = pm25_item.and_then(|i| i.readings.pm25_one_hourly.as_ref());
    let psi_regions = regional(psi_raw);
    let pm25_regions = regional(pm25_raw);
    let psi_value = worst_or_national(&psi_regions, psi_raw);
    let pm25_value = worst_or_national(&pm25_regions, pm25_raw);
    let uv_value = uv.and_then(extract_latest_uv);

    let four_day_entries: Vec<WeatherForecastDay> = four_day
        .and_then(|f| latest_record(&f.data.records, |r| Some(&r.timestamp)))
        .map(|record| {
            let mut entries: Vec<_> = record
                .forecasts
                .iter()
                .filter(|e| parse_timestamp(&e.timestamp).is_some())
                .collect();
            entries.sort_by_key(|e| parse_timestamp(&e.timestamp));
            entries
                .into_iter()
                .take(4)
                .map(|e| WeatherForecastDay {
                    day: e.day.clone(),
                    text: e.forecast.text.clone(),
                    temp_low: e.temperature.low,
                    temp_high: e.temperature.high,
                })
                .collect()
        })
        .unwrap_or_default();

    let readings = |r: Option<&DataGovStationReadingsResponse>| {
        r.and_then(|r| r.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.readings.as_ref())
            .map(|rows| rows.iter().map(|v| v.value).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let temp_values = readings(temperature);
    let humid_values = readings(humidity);

    let timestamps: Vec<Option<&str>> = vec![
        forecast
            .and_then(|f| f.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.timestamps.update_timestamp.as_deref()),
        forecast
            .and_then(|f| f.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.timestamps.timestamp.as_deref()),
        psi_item.and_then(|i| i.timestamps.update_timestamp.as_deref()),
        psi_item.and_then(|i| i.timestamps.timestamp.as_deref()),
        temperature
            .and_then(|r| r.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.timestamps.update_timestamp.as_deref()),
        temperature
            .and_then(|r| r.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.timestamps.timestamp.as_deref()),
        humidity
            .and_then(|r| r.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.timestamps.update_timestamp.as_deref()),
        humidity
            .and_then(|r| r.items.as_ref())
            .and_then(|i| i.first())
            .and_then(|i| i.timestamps.timestamp.as_deref()),
        uv.and_then(|r| r.data.records.first())
            .map(|r| r.updated_timestamp.as_str()),
        uv.and_then(|r| r.data.records.first())
            .map(|r| r.timestamp.as_str()),
        pm25_item.and_then(|i| i.updated_timestamp.as_deref()),
        pm25_item.and_then(|i| i.timestamp.as_deref()),
        four_day
            .and_then(|f| f.data.records.first())
            .map(|r| r.timestamp.as_str()),
    ];

    let data = WeatherData {
        temperature: average(temp_values),
        humidity: average(humid_values),
        psi: psi_value,
        psi_status: get_psi_status(psi_value),
        psi_regions,
        pm25_one_hour: pm25_value,
        pm25_one_hour_band: get_pm25_band(pm25_value),
        pm25_one_hour_regions: pm25_regions,
        uv: uv_value,
        uv_status: get_uv_status(uv_value),
        four_day: four_day_entries,
        forecast: forecast_text,
        last_updated: latest_iso_timestamp(&timestamps),
    };
    (data, partial)
}

impl Client {
    fn data_gov_get<T: DeserializeOwned>(&self, base: &str, endpoint: &str) -> Option<T> {
        self.http
            .get_json(
                "data.gov.sg",
                format!("{base}{endpoint}"),
                &[("Accept", "application/json")],
                DEFAULT_TIMEOUT_MS,
            )
            .ok()
    }

    pub fn get_weather(&self) -> Result<WeatherData> {
        let result = self
            .cache
            .get_with(
                "weather",
                WEATHER_TTL,
                |(_, partial): &(WeatherData, bool)| {
                    if *partial {
                        WEATHER_PARTIAL_TTL
                    } else {
                        WEATHER_TTL
                    }
                },
                || {
                    // Feeds fail independently; fetch them in parallel on scoped
                    // worker threads (synchronous so Rune Latest closures fit).
                    let (forecast, psi, temp, humidity, uv, pm25, four_day) =
                        std::thread::scope(|scope| {
                            let f = scope.spawn(|| {
                                self.data_gov_get(DATA_GOV_BASE_URL, "/2-hour-weather-forecast")
                            });
                            let p = scope.spawn(|| {
                                self.data_gov_get::<DataGovPsiResponse>(DATA_GOV_BASE_URL, "/psi")
                            });
                            let t = scope.spawn(|| {
                                self.data_gov_get::<DataGovStationReadingsResponse>(
                                    DATA_GOV_BASE_URL,
                                    "/air-temperature",
                                )
                            });
                            let h = scope.spawn(|| {
                                self.data_gov_get::<DataGovStationReadingsResponse>(
                                    DATA_GOV_BASE_URL,
                                    "/relative-humidity",
                                )
                            });
                            let u = scope.spawn(|| {
                                self.data_gov_get::<DataGovUvResponse>(DATA_GOV_V2_BASE_URL, "/uv")
                            });
                            let m = scope.spawn(|| {
                                self.data_gov_get::<argus_core::api_types::DataGovPm25Response>(
                                    DATA_GOV_V2_BASE_URL,
                                    "/pm25",
                                )
                            });
                            let d = scope.spawn(|| {
                                self.data_gov_get::<DataGovFourDayResponse>(
                                    DATA_GOV_V2_BASE_URL,
                                    "/four-day-outlook",
                                )
                            });
                            (
                                f.join().unwrap_or(None),
                                p.join().unwrap_or(None),
                                t.join().unwrap_or(None),
                                h.join().unwrap_or(None),
                                u.join().unwrap_or(None),
                                m.join().unwrap_or(None),
                                d.join().unwrap_or(None),
                            )
                        });
                    if forecast.is_none()
                        && psi.is_none()
                        && temp.is_none()
                        && humidity.is_none()
                        && uv.is_none()
                        && pm25.is_none()
                        && four_day.is_none()
                    {
                        return Err(Error::external(
                            "data.gov.sg",
                            502,
                            "All weather endpoints failed",
                        ));
                    }
                    Ok(assemble_weather(
                        forecast.as_ref(),
                        psi.as_ref(),
                        temp.as_ref(),
                        humidity.as_ref(),
                        uv.as_ref(),
                        pm25.as_ref(),
                        four_day.as_ref(),
                    ))
                },
            )
            .map(|(data, _)| data);
        self.finish(Source::Weather, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::api_types::{
        DataGovTimestamps, DataGovUvResponse, ForecastTemperature, ForecastText, FourDayForecast,
        FourDayRecord, Pm25Data, Pm25Item, Pm25Readings, PsiReadingItem, PsiReadings, UvData,
        UvHour, UvRecord, ValueReading,
    };
    use argus_core::types::{Pm25Band, PsiStatus};

    #[test]
    fn uv_status_bands() {
        assert_eq!(get_uv_status(None), UvStatus::Unknown);
        assert_eq!(get_uv_status(Some(2.0)), UvStatus::Low);
        assert_eq!(get_uv_status(Some(5.0)), UvStatus::Moderate);
        assert_eq!(get_uv_status(Some(7.0)), UvStatus::High);
        assert_eq!(get_uv_status(Some(10.0)), UvStatus::VeryHigh);
        assert_eq!(get_uv_status(Some(11.0)), UvStatus::Extreme);
    }

    #[test]
    fn extract_latest_uv_picks_newest_record_then_hour() {
        let response = DataGovUvResponse {
            data: UvData {
                records: vec![
                    UvRecord {
                        updated_timestamp: "2026-01-02T18:00:00Z".into(),
                        timestamp: "2026-01-02T18:00:00Z".into(),
                        index: vec![UvHour {
                            hour: "2026-01-02T18:00:00Z".into(),
                            value: 7.0,
                        }],
                    },
                    UvRecord {
                        updated_timestamp: "2026-01-01T18:00:00Z".into(),
                        timestamp: "2026-01-01T18:00:00Z".into(),
                        index: vec![UvHour {
                            hour: "2026-01-01T18:00:00Z".into(),
                            value: 5.0,
                        }],
                    },
                ],
            },
        };
        assert_eq!(extract_latest_uv(&response), Some(7.0));
    }

    #[test]
    fn assembly_reports_worst_regions_and_partial() {
        let psi = DataGovPsiResponse {
            items: Some(vec![PsiReadingItem {
                timestamps: DataGovTimestamps::default(),
                readings: Some(PsiReadings {
                    psi_twenty_four_hourly: Some(RegionalReading {
                        north: Some(107.0),
                        south: Some(131.0),
                        east: Some(145.0),
                        west: Some(146.0),
                        central: Some(144.0),
                        national: None,
                    }),
                }),
            }]),
        };
        let pm25 = argus_core::api_types::DataGovPm25Response {
            data: Pm25Data {
                items: vec![Pm25Item {
                    timestamp: Some("2026-01-01T00:00:00Z".into()),
                    updated_timestamp: None,
                    readings: Pm25Readings {
                        pm25_one_hourly: Some(RegionalReading {
                            north: Some(88.0),
                            south: Some(173.0),
                            east: Some(160.0),
                            west: Some(195.0),
                            central: None,
                            national: None,
                        }),
                    },
                }],
            },
        };
        let (data, partial) =
            assemble_weather(None, Some(&psi), None, None, None, Some(&pm25), None);
        assert!(partial);
        assert_eq!(data.psi, Some(146.0));
        assert_eq!(data.psi_status, PsiStatus::Unhealthy);
        assert_eq!(data.pm25_one_hour, Some(195.0));
        assert_eq!(data.pm25_one_hour_band, Pm25Band::High);
        assert_eq!(data.pm25_one_hour_regions.central, None);
        assert_eq!(data.forecast, "No forecast available");
    }

    #[test]
    fn four_day_outlook_is_capped_at_four() {
        let record = FourDayRecord {
            timestamp: "2026-01-01T17:00:00Z".into(),
            forecasts: (1..=5)
                .map(|i| FourDayForecast {
                    day: format!("Day {i}"),
                    timestamp: format!("2026-01-0{}T00:00:00Z", i + 1),
                    forecast: ForecastText {
                        text: "Cloudy".into(),
                    },
                    temperature: ForecastTemperature {
                        low: 25.0,
                        high: 32.0,
                    },
                })
                .collect(),
        };
        let four = DataGovFourDayResponse {
            data: argus_core::api_types::FourDayData {
                records: vec![record],
            },
        };
        let temp = DataGovStationReadingsResponse {
            items: Some(vec![argus_core::api_types::StationReadingsItem {
                timestamps: DataGovTimestamps::default(),
                readings: Some(vec![
                    ValueReading { value: 28.0 },
                    ValueReading { value: 30.0 },
                ]),
            }]),
        };
        let (data, _) = assemble_weather(None, None, Some(&temp), None, None, None, Some(&four));
        assert_eq!(data.four_day.len(), 4);
        assert_eq!(data.four_day[0].day, "Day 1");
        assert_eq!(data.temperature, Some(29.0));
    }
}
