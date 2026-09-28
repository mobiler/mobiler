package dev.mobiler.barbershop.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import dev.mobiler.barbershop.shared.types.FontFamily as ModelFontFamily

private val baseBody = TextStyle(
    fontWeight = FontWeight.Normal,
    fontSize = 16.sp,
    lineHeight = 24.sp,
    letterSpacing = 0.5.sp,
)

/** A synced font role (`mobiler fonts sync` → res/font/mobiler_<role>_<weight>), looked up by name so
 *  the shell compiles without it; null when the app synced none (the system font is used). */
fun syncedFamily(context: android.content.Context, role: String): FontFamily? =
    syncedFamilies.getOrPut(role) { lookUpFamily(context, role) }

// Resources don't change while the app runs, so each role is looked up once.
private val syncedFamilies = mutableMapOf<String, FontFamily?>()

private fun lookUpFamily(context: android.content.Context, role: String): FontFamily? {
    val fonts = (100..900 step 100).mapNotNull { w ->
        val id = context.resources.getIdentifier("mobiler_${role}_$w", "font", context.packageName)
        if (id != 0) androidx.compose.ui.text.font.Font(id, FontWeight(w)) else null
    }
    return if (fonts.isEmpty()) null else FontFamily(fonts)
}

/// `Custom`: the display family on the headline/title slots (widget Title/Subtitle, app bar, sheet
/// titles) and the body family on body/label slots; a role without synced files → the system font.
fun typographyFor(font: ModelFontFamily?, context: android.content.Context): Typography {
    if (font == ModelFontFamily.CUSTOM) {
        val display = syncedFamily(context, "display") ?: FontFamily.Default
        val body = syncedFamily(context, "body") ?: FontFamily.Default
        val t = Typography()
        return t.copy(
            displayLarge = t.displayLarge.copy(fontFamily = display),
            displayMedium = t.displayMedium.copy(fontFamily = display),
            displaySmall = t.displaySmall.copy(fontFamily = display),
            headlineLarge = t.headlineLarge.copy(fontFamily = display),
            headlineMedium = t.headlineMedium.copy(fontFamily = display),
            headlineSmall = t.headlineSmall.copy(fontFamily = display),
            titleLarge = t.titleLarge.copy(fontFamily = display),
            titleMedium = t.titleMedium.copy(fontFamily = display),
            titleSmall = t.titleSmall.copy(fontFamily = body),
            bodyLarge = baseBody.copy(fontFamily = body),
            bodyMedium = t.bodyMedium.copy(fontFamily = body),
            bodySmall = t.bodySmall.copy(fontFamily = body),
            labelLarge = t.labelLarge.copy(fontFamily = body),
            labelMedium = t.labelMedium.copy(fontFamily = body),
            labelSmall = t.labelSmall.copy(fontFamily = body),
        )
    }
    return typographyFor(font)
}

/// The Material3 `Typography` for a theme's font choice. Android has no native "rounded"
/// system font, so Rounded falls back to SansSerif; Serif/Monospace map natively. `null`
/// (un-themed) keeps the default sans body — the original look.
fun typographyFor(font: ModelFontFamily?): Typography {
    val family = when (font) {
        ModelFontFamily.SERIF -> FontFamily.Serif
        ModelFontFamily.MONOSPACE -> FontFamily.Monospace
        // Rounded has no AOSP system equivalent; SansSerif is the closest default.
        ModelFontFamily.ROUNDED, ModelFontFamily.SYSTEM, ModelFontFamily.CUSTOM, null -> FontFamily.Default
    }
    return Typography(bodyLarge = baseBody.copy(fontFamily = family))
}
