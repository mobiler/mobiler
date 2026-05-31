package dev.mobiler.coffee.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import dev.mobiler.coffee.shared.types.FontFamily as ModelFontFamily

private val baseBody = TextStyle(
    fontWeight = FontWeight.Normal,
    fontSize = 16.sp,
    lineHeight = 24.sp,
    letterSpacing = 0.5.sp,
)

/// The Material3 `Typography` for a theme's font choice. Android has no native "rounded"
/// system font, so Rounded falls back to SansSerif; Serif/Monospace map natively. `null`
/// (un-themed) keeps the default sans body — the original look.
fun typographyFor(font: ModelFontFamily?): Typography {
    val family = when (font) {
        ModelFontFamily.SERIF -> FontFamily.Serif
        ModelFontFamily.MONOSPACE -> FontFamily.Monospace
        // Rounded has no AOSP system equivalent; SansSerif is the closest default.
        ModelFontFamily.ROUNDED, ModelFontFamily.SYSTEM, null -> FontFamily.Default
    }
    return Typography(bodyLarge = baseBody.copy(fontFamily = family))
}
