use crate::model::{ExactNumber, FrameCount};

pub(super) fn zoom_filter(by: &ExactNumber, frames: FrameCount) -> String {
    let last_frame = frames.0.saturating_sub(1).max(1);
    let zoom_in = format!(
        "(1+{}*(in-1)/({}*{last_frame}))",
        by.numerator(),
        by.denominator()
    );
    let x_margin = format!("W*(1-1/{zoom_in})/2");
    let y_margin = format!("H*(1-1/{zoom_in})/2");
    format!(
        "perspective=x0='{x_margin}':y0='{y_margin}':x1='W-{x_margin}':y1='{y_margin}':x2='{x_margin}':y2='H-{y_margin}':x3='W-{x_margin}':y3='H-{y_margin}':sense=source:eval=frame:interpolation=cubic,setpts=PTS-STARTPTS"
    )
}
