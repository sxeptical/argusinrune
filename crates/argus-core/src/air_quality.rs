//! NEA air-quality bands. Ported from `lib/air-quality.ts`.
//! Pure and dependency-free so the API client, panels and digest agree.

use crate::types::{AirQualityTone, Pm25Band, PsiStatus};

pub struct PsiBand {
    pub status: PsiStatus,
    pub range: &'static str,
    pub max: f64,
}

pub const PSI_BANDS: &[PsiBand] = &[
    PsiBand { status: PsiStatus::Good, range: "0–50", max: 50.0 },
    PsiBand { status: PsiStatus::Moderate, range: "51–100", max: 100.0 },
    PsiBand { status: PsiStatus::Unhealthy, range: "101–200", max: 200.0 },
    PsiBand { status: PsiStatus::VeryUnhealthy, range: "201–300", max: 300.0 },
    PsiBand { status: PsiStatus::Hazardous, range: "301+", max: f64::INFINITY },
];

pub struct Pm25BandInfo {
    pub band: Pm25Band,
    pub label: &'static str,
    pub range: &'static str,
    pub max: f64,
}

pub const PM25_BANDS: &[Pm25BandInfo] = &[
    Pm25BandInfo { band: Pm25Band::Normal, label: "I · Normal", range: "0–55", max: 55.0 },
    Pm25BandInfo { band: Pm25Band::Elevated, label: "II · Elevated", range: "56–150", max: 150.0 },
    Pm25BandInfo { band: Pm25Band::High, label: "III · High", range: "151–250", max: 250.0 },
    Pm25BandInfo { band: Pm25Band::VeryHigh, label: "IV · Very High", range: "251+", max: f64::INFINITY },
];

pub fn get_psi_status(psi: Option<f64>) -> PsiStatus {
    let psi = match psi {
        Some(p) if p.is_finite() => p,
        _ => return PsiStatus::Unknown,
    };
    PSI_BANDS
        .iter()
        .find(|band| psi <= band.max)
        .map(|band| band.status.clone())
        .unwrap_or(PsiStatus::Hazardous)
}

pub fn get_pm25_band(pm25: Option<f64>) -> Pm25Band {
    let pm25 = match pm25 {
        Some(p) if p.is_finite() => p,
        _ => return Pm25Band::Unknown,
    };
    PM25_BANDS
        .iter()
        .find(|band| pm25 <= band.max)
        .map(|band| band.band.clone())
        .unwrap_or(Pm25Band::VeryHigh)
}

pub fn psi_tone(status: &PsiStatus) -> AirQualityTone {
    match status {
        PsiStatus::Good => AirQualityTone::Success,
        PsiStatus::Moderate => AirQualityTone::Warning,
        PsiStatus::Unknown => AirQualityTone::Muted,
        _ => AirQualityTone::Danger,
    }
}

pub fn pm25_tone(band: &Pm25Band) -> AirQualityTone {
    match band {
        Pm25Band::Normal => AirQualityTone::Success,
        Pm25Band::Elevated => AirQualityTone::Warning,
        Pm25Band::Unknown => AirQualityTone::Muted,
        _ => AirQualityTone::Danger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bands_and_tones() {
        assert_eq!(get_psi_status(Some(45.0)), PsiStatus::Good);
        assert_eq!(get_psi_status(Some(150.0)), PsiStatus::Unhealthy);
        assert_eq!(get_psi_status(Some(999.0)), PsiStatus::Hazardous);
        assert_eq!(get_psi_status(None), PsiStatus::Unknown);
        assert_eq!(get_pm25_band(Some(200.0)), Pm25Band::High);
        assert_eq!(psi_tone(&PsiStatus::Good), AirQualityTone::Success);
        assert_eq!(pm25_tone(&Pm25Band::VeryHigh), AirQualityTone::Danger);
    }
}
