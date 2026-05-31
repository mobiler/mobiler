package dev.mobiler.todo.ui.theme

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
import dev.mobiler.todo.shared.types.Corner
import dev.mobiler.todo.shared.types.Theme as ModelTheme

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
fun TodoTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    // App branding (theme-as-data). null → framework defaults (dynamic color on 12+, else Purple).
    theme: ModelTheme? = null,
    // Dynamic color is available on Android 12+ — but a brand `theme` overrides it (else the
    // wallpaper palette would ignore the app's brand color).
    dynamicColor: Boolean = true,
    content: @Composable () -> Unit,
) {
    val colorScheme = when {
        // A brand theme wins: build a scheme seeded from its color, no dynamic override.
        theme != null -> {
            val seed = Color(theme.seed.r.toInt(), theme.seed.g.toInt(), theme.seed.b.toInt())
            val base = if (darkTheme) DarkColorScheme else LightColorScheme
            base.copy(primary = seed, secondary = seed, tertiary = seed)
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
