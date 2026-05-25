package dev.mobiler.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.StarOutline
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.mobiler.app.ui.theme.MobilerTheme
import dev.mobiler.shared.types.ButtonStyle
import dev.mobiler.shared.types.CardStyle
import dev.mobiler.shared.types.Event
import dev.mobiler.shared.types.Icon as WidgetIcon
import dev.mobiler.shared.types.ProjectColor
import dev.mobiler.shared.types.Spacing
import dev.mobiler.shared.types.TextStyle as ModelTextStyle
import dev.mobiler.shared.types.Tone
import dev.mobiler.shared.types.Widget

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            App()
        }
    }
}

@Composable
fun App(core: Core = viewModel()) {
    // The theme wrap lives INSIDE App so it can read `dark_mode` from the Rust-built
    // ViewModel. Theme is data: it crosses the FFI like everything else.
    val view = core.view
    val darkMode = (view as? Widget.Scaffold)?.darkMode ?: isSystemInDarkTheme()
    MobilerTheme(darkTheme = darkMode, dynamicColor = false) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background,
        ) {
            Render(view) { event -> core.update(event) }
        }
    }
}

/**
 * Generic Render — knows nothing about the app, just maps Widget variants to
 * Compose primitives. Style intent (TextStyle, ButtonStyle, CardStyle, Tone,
 * Spacing, Icon) is decided in Rust; the concrete look (font tokens, colors,
 * dp values) is decided here.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Render(widget: Widget, send: (Event) -> Unit) {
    when (widget) {
        is Widget.Text -> Text(
            text = widget.content,
            style = typographyFor(widget.style),
            fontWeight = if (widget.style == ModelTextStyle.EMPHASIS) FontWeight.Medium else null,
            color = colorFor(widget.style),
            modifier = Modifier.padding(vertical = 2.dp),
        )

        is Widget.Spacer -> Spacer(modifier = Modifier.height(spacingFor(widget.size)))

        is Widget.Divider -> HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

        is Widget.Row -> Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            widget.children.forEach { RowItem(it, send) }
        }

        is Widget.Column -> Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            widget.children.forEach { Render(it, send) }
        }

        is Widget.Card -> when (widget.style) {
            CardStyle.ELEVATED -> Card(
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
            ) { CardBody(widget.child, send) }
            CardStyle.OUTLINED -> OutlinedCard(
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
            ) { CardBody(widget.child, send) }
            CardStyle.FILLED -> Card(
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceVariant,
                ),
            ) { CardBody(widget.child, send) }
        }

        is Widget.Button -> when (widget.style) {
            ButtonStyle.FILLED -> Button(onClick = { send(widget.onPress) }) {
                Text(text = widget.label)
            }
            ButtonStyle.OUTLINED -> OutlinedButton(onClick = { send(widget.onPress) }) {
                Text(text = widget.label)
            }
            ButtonStyle.TEXT -> TextButton(onClick = { send(widget.onPress) }) {
                Text(text = widget.label)
            }
        }

        is Widget.IconButton -> IconButton(onClick = { send(widget.onPress) }) {
            Icon(
                imageVector = iconFor(widget.icon),
                contentDescription = widget.icon.name.lowercase(),
                tint = iconTintFor(widget.icon),
            )
        }

        is Widget.Badge -> {
            val (bg, fg) = toneColors(widget.tone)
            Box(
                modifier = Modifier
                    .background(color = bg, shape = RoundedCornerShape(50))
                    .padding(horizontal = 12.dp, vertical = 4.dp),
            ) {
                Text(
                    text = widget.label,
                    style = MaterialTheme.typography.labelMedium,
                    color = fg,
                )
            }
        }

        is Widget.ColorDot -> Box(
            modifier = Modifier
                .size(12.dp)
                .background(color = projectColorOf(widget.color), shape = CircleShape),
        )

        is Widget.ColorSwatch -> Box(
            modifier = Modifier
                .size(32.dp)
                .clip(CircleShape)
                .background(color = projectColorOf(widget.color), shape = CircleShape)
                .then(
                    if (widget.selected) {
                        Modifier.border(
                            width = 3.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                            shape = CircleShape,
                        )
                    } else {
                        Modifier
                    },
                )
                .clickable { send(widget.onPress) },
        )

        is Widget.TextField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { newValue ->
                send(Event.TextChanged(id = widget.id, value = newValue))
            },
            placeholder = { Text(widget.placeholder) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        is Widget.Switch -> Row(
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(text = widget.label, modifier = Modifier.weight(1f))
            Switch(
                checked = widget.value,
                onCheckedChange = { newValue ->
                    send(Event.Toggled(id = widget.id, value = newValue))
                },
            )
        }

        is Widget.Checkbox -> Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(
                checked = widget.value,
                onCheckedChange = { send(widget.onChange) },
            )
            Text(text = widget.label, modifier = Modifier.weight(1f))
        }

        is Widget.Scaffold -> Scaffold(
            modifier = Modifier.fillMaxSize(),
            topBar = {
                CenterAlignedTopAppBar(
                    title = { Text(widget.title) },
                    navigationIcon = {
                        val back = widget.backAction
                        if (back != null) {
                            IconButton(onClick = { send(back) }) {
                                Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                            }
                        }
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                        containerColor = MaterialTheme.colorScheme.surface,
                    ),
                )
            },
            bottomBar = {
                NavigationBar {
                    widget.bottomTabs.forEach { tab ->
                        NavigationBarItem(
                            selected = tab.key == widget.activeTab,
                            onClick = { send(Event.SelectTab(tab.key)) },
                            label = { Text(tab.label) },
                            icon = {},
                        )
                    }
                }
            },
        ) { padding ->
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(horizontal = 16.dp, vertical = 8.dp)
                    .verticalScroll(rememberScrollState()),
            ) {
                Render(widget.body, send)
            }
        }
    }
}

// ---------- Style-token mappings (the ONLY place that decides concrete looks) ----------

@Composable
private fun typographyFor(style: ModelTextStyle): androidx.compose.ui.text.TextStyle = when (style) {
    ModelTextStyle.BODY -> MaterialTheme.typography.bodyLarge
    ModelTextStyle.TITLE -> MaterialTheme.typography.headlineMedium
    ModelTextStyle.SUBTITLE -> MaterialTheme.typography.titleMedium
    ModelTextStyle.CAPTION -> MaterialTheme.typography.bodySmall
    ModelTextStyle.EMPHASIS -> MaterialTheme.typography.bodyLarge
}

@Composable
private fun colorFor(style: ModelTextStyle): Color = when (style) {
    ModelTextStyle.CAPTION -> MaterialTheme.colorScheme.onSurfaceVariant
    else -> MaterialTheme.colorScheme.onSurface
}

private fun spacingFor(size: Spacing): Dp = when (size) {
    Spacing.XS -> 4.dp
    Spacing.SM -> 8.dp
    Spacing.MD -> 12.dp
    Spacing.LG -> 16.dp
    Spacing.XL -> 24.dp
}

private fun iconFor(icon: WidgetIcon): androidx.compose.ui.graphics.vector.ImageVector = when (icon) {
    WidgetIcon.DELETE -> Icons.Default.Delete
    WidgetIcon.ADD -> Icons.Default.Add
    WidgetIcon.EDIT -> Icons.Default.Edit
    WidgetIcon.CLOSE -> Icons.Default.Close
    WidgetIcon.SETTINGS -> Icons.Default.Settings
    WidgetIcon.CHECK -> Icons.Default.Check
    WidgetIcon.STAR -> Icons.Default.Star
    WidgetIcon.STAROUTLINE -> Icons.Outlined.StarOutline
}

@Composable
private fun iconTintFor(icon: WidgetIcon): Color = when (icon) {
    // Filled star gets the primary tint to visually "pop" as a marked state.
    WidgetIcon.STAR -> MaterialTheme.colorScheme.primary
    else -> androidx.compose.material3.LocalContentColor.current
}

/// Pair of (background, foreground) colors for a given semantic tone.
/// We pick from the theme's colorScheme so dark mode comes for free.
@Composable
private fun toneColors(tone: Tone): Pair<Color, Color> {
    val cs = MaterialTheme.colorScheme
    return when (tone) {
        Tone.NEUTRAL -> cs.surfaceVariant to cs.onSurfaceVariant
        Tone.SUCCESS -> Color(0xFF2E7D32).copy(alpha = 0.15f) to Color(0xFF2E7D32)
        Tone.WARNING -> Color(0xFFE65100).copy(alpha = 0.15f) to Color(0xFFE65100)
        Tone.DANGER -> cs.errorContainer to cs.onErrorContainer
        Tone.INFO -> cs.tertiaryContainer to cs.onTertiaryContainer
    }
}

/// Concrete RGB for each project-identity color. The Rust core stays abstract
/// (`ProjectColor::Indigo`); this render layer owns the actual hues. Same values
/// in light and dark — these are saturated identity colors, not theme surfaces.
private fun projectColorOf(color: ProjectColor): Color = when (color) {
    ProjectColor.INDIGO -> Color(0xFF5C6BC0)
    ProjectColor.TEAL -> Color(0xFF26A69A)
    ProjectColor.CORAL -> Color(0xFFFF7043)
    ProjectColor.AMBER -> Color(0xFFFFB300)
    ProjectColor.LIME -> Color(0xFF9CCC65)
    ProjectColor.PINK -> Color(0xFFEC407A)
}

@Composable
private fun CardBody(child: Widget, send: (Event) -> Unit) {
    Box(modifier = Modifier.padding(16.dp)) {
        Render(child, send)
    }
}

@Composable
private fun RowScope.RowItem(widget: Widget, send: (Event) -> Unit) {
    when (widget) {
        is Widget.TextField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { send(Event.TextChanged(id = widget.id, value = it)) },
            placeholder = { Text(widget.placeholder) },
            singleLine = true,
            modifier = Modifier.weight(1f),
        )
        is Widget.Checkbox -> Row(
            modifier = Modifier.weight(1f),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(checked = widget.value, onCheckedChange = { send(widget.onChange) })
            Text(text = widget.label)
        }
        is Widget.Text -> Text(
            text = widget.content,
            style = typographyFor(widget.style),
            color = colorFor(widget.style),
            modifier = Modifier.weight(1f),
        )
        is Widget.Column -> Column(modifier = Modifier.weight(1f)) {
            widget.children.forEach { Render(it, send) }
        }
        else -> Render(widget, send)
    }
}
