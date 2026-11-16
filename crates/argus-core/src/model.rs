//! Skeleton domain model. Filled out as features are ported from the TS app.

use serde::{Deserialize, Serialize};

/// Stable identifier for a map layer (e.g. `flights`, `news`).
pub type LayerId = String;

/// A toggleable overlay on the map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub label: String,
    pub visible: bool,
}

impl Layer {
    pub fn new(id: impl Into<LayerId>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), visible: false }
    }
}

/// Return the labels of enabled layers, in declaration order.
pub fn enabled_layer_labels(layers: &[Layer]) -> Vec<&str> {
    layers.iter().filter(|l| l.visible).map(|l| l.label.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_visible_layers_are_reported() {
        let mut layers = vec![Layer::new("flights", "Flights"), Layer::new("news", "News")];
        layers[0].visible = true;
        assert_eq!(enabled_layer_labels(&layers), ["Flights"]);
    }
}
