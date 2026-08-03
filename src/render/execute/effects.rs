use crate::model::FrameCount;
use crate::preflight::PreparedZoomCurve;

pub(super) fn zoom_filter(curve: &PreparedZoomCurve, frames: FrameCount) -> String {
    let last_frame = frames.0.saturating_sub(1).max(1);
    let zoom_in = curve
        .amounts()
        .into_iter()
        .map(|by| {
            format!(
                "(1+{}*(in-1)/({}*{last_frame}))",
                by.numerator(),
                by.denominator()
            )
        })
        .collect::<Vec<_>>()
        .join("*");
    let zoom_in = format!("({zoom_in})");
    let x_margin = format!("W*(1-1/{zoom_in})/2");
    let y_margin = format!("H*(1-1/{zoom_in})/2");
    format!(
        "perspective=x0='{x_margin}':y0='{y_margin}':x1='W-{x_margin}':y1='{y_margin}':x2='{x_margin}':y2='H-{y_margin}':x3='W-{x_margin}':y3='H-{y_margin}':sense=source:eval=frame:interpolation=cubic,setpts=PTS-STARTPTS"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ExactNumber;

    #[test]
    fn composed_zoom_parenthesizes_the_complete_scale_product() {
        let curve = PreparedZoomCurve::new(ExactNumber::from_ratio(1, 10))
            .expect("first amount")
            .appended(ExactNumber::from_ratio(1, 5))
            .expect("second amount");

        let filter = zoom_filter(&curve, FrameCount(11));

        assert!(filter.contains("1/((1+1*(in-1)/(10*10))*(1+1*(in-1)/(5*10)))"));
    }

    #[test]
    fn prepared_zoom_filter_size_estimate_is_conservative() {
        let mut curve = PreparedZoomCurve::new(ExactNumber::from_ratio(123_456_789, 987_654_321))
            .expect("first amount");
        for _ in 1..100 {
            curve = curve
                .appended(ExactNumber::from_ratio(123_456_789, 987_654_321))
                .expect("next amount");
        }
        let frames = FrameCount(1_000_000);
        let filter = zoom_filter(&curve, frames);

        assert!(
            curve.estimated_filter_bytes(frames) >= filter.len(),
            "estimated {} bytes for a {}-byte filter",
            curve.estimated_filter_bytes(frames),
            filter.len(),
        );
    }
}
