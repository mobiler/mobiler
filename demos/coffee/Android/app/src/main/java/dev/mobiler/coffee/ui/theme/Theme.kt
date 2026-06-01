package dev.mobiler.coffee.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.mobiler.coffee.shared.types.Corner
import dev.mobiler.coffee.shared.types.Theme as ModelTheme

private val DarkColorScheme = darkColorScheme(
    primary = TerracottaLight,
    onPrimary = TerracottaOnPrimaryDark,
    primaryContainer = TerracottaContainerDark,
    onPrimaryContainer = TerracottaOnContainerDark,
    secondary = TaupeLight,
    tertiary = OliveLight,
)

private val LightColorScheme = lightColorScheme(
    primary = Terracotta,
    onPrimary = TerracottaOnPrimary,
    primaryContainer = TerracottaContainer,
    onPrimaryContainer = TerracottaOnContainer,
    secondary = Taupe,
    tertiary = Olive,
)

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
fun CoffeeTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    // App branding (theme-as-data). null → the terracotta brand below.
    theme: ModelTheme? = null,
    // Off by default so the terracotta brand shows instead of the device palette.
    dynamicColor: Boolean = false,
    content: @Composable () -> Unit,
) {
    val colorScheme = when {
        // A brand theme wins: build a scheme seeded from its color, over the terracotta default.
        theme != null -> {
            val seed = Color(theme.seed.r.toInt(), theme.seed.g.toInt(), theme.seed.b.toInt())
            val base = if (darkTheme) DarkColorScheme else LightColorScheme
            val accent = theme.accent?.let { Color(it.r.toInt(), it.g.toInt(), it.b.toInt()) } ?: seed
            base.copy(primary = seed, secondary = accent, tertiary = accent)
        }
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColorScheme
        else -> LightColorScheme
    }

    MaterialTheme(
        colorScheme = colorScheme,
        typography = typographyFor(theme?.font),
        shapes = theme?.let { shapesFor(it.corner) } ?: Shapes(),
        content = content,
    )
}
