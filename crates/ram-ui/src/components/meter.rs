//! Audio level meter component.

use leptos::prelude::*;

/// Audio level meter displaying dBFS levels.
#[component]
pub fn Meter(
    /// Current level in dBFS (-60 to 0).
    #[prop(into)]
    level: Signal<f32>,
    /// Peak hold level in dBFS (optional).
    #[prop(optional)]
    peak: Option<Signal<f32>>,
    /// Orientation: "horizontal" or "vertical".
    #[prop(default = "vertical")]
    orientation: &'static str,
    /// Show numeric value.
    #[prop(default = false)]
    show_value: bool,
) -> impl IntoView {
    // Convert dBFS to percentage (0-100)
    // -60 dB = 0%, 0 dB = 100%
    let level_percent = move || {
        let db = level.get();
        ((db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0)
    };

    let peak_percent = move || {
        peak.map(|p| {
            let db = p.get();
            ((db + 60.0) / 60.0 * 100.0).clamp(0.0, 100.0)
        })
    };

    // Color based on level
    let meter_color = move || {
        let db = level.get();
        if db > -3.0 {
            "meter-clip" // Red - clipping
        } else if db > -12.0 {
            "meter-hot" // Yellow - hot
        } else {
            "meter-normal" // Green - normal
        }
    };

    let style = move || {
        if orientation == "horizontal" {
            format!("width: {}%", level_percent())
        } else {
            format!("height: {}%", level_percent())
        }
    };

    let peak_style = move || {
        peak_percent().map(|p| {
            if orientation == "horizontal" {
                format!("left: {}%", p)
            } else {
                format!("bottom: {}%", p)
            }
        })
    };

    view! {
        <div class=format!("meter meter-{}", orientation)>
            <div class="meter-track">
                <div
                    class=move || format!("meter-fill {}", meter_color())
                    style=style
                />
                {move || peak_style().map(|s| view! {
                    <div class="meter-peak" style=s/>
                })}
            </div>
            {show_value.then(|| view! {
                <span class="meter-value">
                    {move || format!("{:.1}", level.get())}
                    " dB"
                </span>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn meter_compiles() {
        // Compilation test
    }

    #[test]
    fn db_to_percent_conversion() {
        // -60 dB -> 0%
        // -30 dB -> 50%
        // 0 dB -> 100%
        assert_eq!((-60.0_f32 + 60.0) / 60.0 * 100.0, 0.0);
        assert_eq!((-30.0_f32 + 60.0) / 60.0 * 100.0, 50.0);
        assert_eq!((0.0_f32 + 60.0) / 60.0 * 100.0, 100.0);
    }
}
