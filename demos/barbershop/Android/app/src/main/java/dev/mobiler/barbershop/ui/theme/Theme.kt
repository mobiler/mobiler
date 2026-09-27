package dev.mobiler.barbershop.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.mobiler.barbershop.shared.types.ColorRoles
import dev.mobiler.barbershop.shared.types.Corner
import dev.mobiler.barbershop.shared.types.Rgb
import dev.mobiler.barbershop.shared.types.Rgba
import dev.mobiler.barbershop.shared.types.Theme as ModelTheme

private val DarkColorScheme = darkColorScheme(
    primary = Purple80,
    secondary = PurpleGrey80,
    tertiary = Pink80,
)

private val LightColorScheme = lightColorScheme(
    primary = Purple40,
    secondary = PurpleGrey40,
    tertiary = Pink40,
)

/** The active palette set (light or dark by `darkTheme`); null = no palette, every widget keeps its colours. */
val LocalPalette = staticCompositionLocalOf<ColorRoles?> { null }
fun Rgb.color() = Color(r.toInt(), g.toInt(), b.toInt())
fun Rgba.color() = Color(r.toInt(), g.toInt(), b.toInt(), a.toInt())

/** Every M3 slot a palette role maps to; an unset role keeps `base`'s slot. Cards, sheets and dialogs
 *  read the surfaceContainer* slots by default, the FAB reads primaryContainer. */
private fun paletteScheme(base: ColorScheme, p: ColorRoles): ColorScheme {
    val surface = p.surface?.color()
    return base.copy(
        background = p.background?.color() ?: base.background,
        onBackground = p.onSurface?.color() ?: base.onBackground,
        surface = surface ?: base.surface,
        onSurface = p.onSurface?.color() ?: base.onSurface,
        surfaceVariant = p.surfaceMuted?.color() ?: base.surfaceVariant,
        onSurfaceVariant = p.onSurfaceVariant?.color() ?: base.onSurfaceVariant,
        surfaceContainerLowest = surface ?: base.surfaceContainerLowest,
        surfaceContainerLow = surface ?: base.surfaceContainerLow,
        surfaceContainer = p.surfaceBar?.color() ?: base.surfaceContainer,
        surfaceContainerHigh = surface ?: base.surfaceContainerHigh,
        surfaceContainerHighest = surface ?: base.surfaceContainerHighest,
        outline = p.outline?.color() ?: base.outline,
        outlineVariant = p.outlineVariant?.color() ?: base.outlineVariant,
        primary = p.primary?.color() ?: base.primary,
        onPrimary = p.onPrimary?.color() ?: base.onPrimary,
        secondaryContainer = p.secondaryContainer?.color() ?: base.secondaryContainer,
        onSecondaryContainer = p.onSecondaryContainer?.color() ?: base.onSecondaryContainer,
        primaryContainer = p.fab?.color() ?: base.primaryContainer,
        onPrimaryContainer = p.onFab?.color() ?: base.onPrimaryContainer,
        error = p.danger?.onContainer?.color() ?: base.error,
        onError = p.danger?.container?.color() ?: base.onError,
        errorContainer = p.danger?.container?.color() ?: base.errorContainer,
        onErrorContainer = p.danger?.onContainer?.color() ?: base.onErrorContainer,
    )
}

/// Maps a model `Corner` to a Material3 `Shapes` set (small/medium/large component corners).
private fun shapesFor(corner: Corner): Shapes {
    val r = when (corner) {
        Corner.NONE -> 0
        Corner.SMALL -> 8
        Corner.MEDIUM -> 14
        Corner.LARGE -> 22
    }
    return Shapes(
        small = RoundedCornerShape((r - 4).coerceAtLeast(0).dp),
        medium = RoundedCornerShape(r.dp),
        large = RoundedCornerShape((r + 6).dp),
    )
}

@Composable
fun FadehouseTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    // App branding (theme-as-data). null → framework defaults (dynamic color on 12+, else Purple).
    theme: ModelTheme? = null,
    // Dynamic color is available on Android 12+ — but a brand `theme` overrides it (else the
    // wallpaper palette would ignore the app's brand color).
    dynamicColor: Boolean = true,
    content: @Composable () -> Unit,
) {
    val roles = theme?.palette?.let { if (darkTheme) it.dark else it.light }
    val colorScheme = when {
        // A brand theme wins: build a scheme seeded from its color, no dynamic override.
        theme != null -> {
            val seed = Color(theme.seed.r.toInt(), theme.seed.g.toInt(), theme.seed.b.toInt())
            // Optional accent seeds M3 secondary/tertiary (drives CardStyle.BRAND gradients); falls back to the seed.
            val accent = theme.accent?.let { Color(it.r.toInt(), it.g.toInt(), it.b.toInt()) } ?: seed
            val base = if (darkTheme) DarkColorScheme else LightColorScheme
            val themed = base.copy(primary = seed, secondary = accent, tertiary = accent)
            roles?.let { paletteScheme(themed, it) } ?: themed
        }
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColorScheme
        else -> LightColorScheme
    }
    CompositionLocalProvider(LocalPalette provides roles) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = typographyFor(theme?.font),
            shapes = theme?.let { shapesFor(it.corner) } ?: Shapes(),
            content = content,
        )
    }
}
