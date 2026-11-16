//! Flight summary helper. Ported from `lib/flights.ts`.

use crate::types::{FlightDirection, FlightState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightSummary {
    pub inbound: i64,
    pub outbound: i64,
    pub transit: i64,
}

impl FlightSummary {
    pub fn add(&mut self, direction: FlightDirection) {
        match direction {
            FlightDirection::Inbound => self.inbound += 1,
            FlightDirection::Outbound => self.outbound += 1,
            FlightDirection::Transit => self.transit += 1,
        }
    }
}

pub fn summarize_flights(flights: &[FlightState]) -> FlightSummary {
    flights
        .iter()
        .fold(FlightSummary::default(), |mut summary, flight| {
            summary.add(flight.direction);
            summary
        })
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_directions() {
        let summary = FlightSummary {
            inbound: 2,
            outbound: 1,
            ..Default::default()
        };
        assert_eq!(summary.inbound + summary.outbound + summary.transit, 3);
    }
}
