package {{PACKAGE}}

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import coil3.compose.AsyncImage
import {{PACKAGE}}.ui.theme.{{NAME}}Theme
import {{PACKAGE_SHARED_TYPES}}.BoxAlign
import {{PACKAGE_SHARED_TYPES}}.ButtonStyle
import {{PACKAGE_SHARED_TYPES}}.CardStyle
import {{PACKAGE_SHARED_TYPES}}.Event
import {{PACKAGE_SHARED_TYPES}}.Icon as WidgetIcon
import {{PACKAGE_SHARED_TYPES}}.ImageRatio
import {{PACKAGE_SHARED_TYPES}}.ImageShape
import {{PACKAGE_SHARED_TYPES}}.Spacing
import {{PACKAGE_SHARED_TYPES}}.TextStyle as ModelTextStyle
import {{PACKAGE_SHARED_TYPES}}.Tone
import {{PACKAGE_SHARED_TYPES}}.Widget

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            {{NAME}}Theme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    App()
                }
            }
        }
    }
}

@Composable
fun App(core: Core = viewModel()) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Render(core.view) { event -> core.update(event) }
    }
}

/**
 * Generic Render — maps Widget variants to Compose. Style *intent* (TextStyle,
 * ButtonStyle, CardStyle, Tone, Spacing, Icon, ImageShape/Ratio, BoxAlign) is
 * decided in Rust; the concrete look (fonts, colors, dp, shapes) is decided here.
 * This file only changes when a brand-new Widget kind is added.
 */
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

        is Widget.Image -> AsyncImage(
            model = widget.source,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(ratioFor(widget.ratio))
                .clip(shapeFor(widget.shape)),
        )

        is Widget.Badge -> {
            val (bg, fg) = toneColors(widget.tone)
            Box(
                modifier = Modifier
                    .background(color = bg, shape = RoundedCornerShape(50))
                    .padding(horizontal = 12.dp, vertical = 4.dp),
            ) {
                Text(text = widget.label, style = MaterialTheme.typography.labelMedium, color = fg)
            }
        }

        is Widget.Divider -> HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

        is Widget.Spacer -> Spacer(modifier = Modifier.height(spacingFor(widget.size)))

        is Widget.Row -> Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            widget.children.forEach { RowItem(it, send) }
        }

        is Widget.Column -> Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            widget.children.forEach { Render(it, send) }
        }

        is Widget.Card -> when (widget.style) {
            CardStyle.ELEVATED -> Card(
                modifier = Modifier.fillMaxWidth(),
                elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
            ) { CardBody(widget.child, send) }
            CardStyle.OUTLINED -> OutlinedCard(modifier = Modifier.fillMaxWidth()) {
                CardBody(widget.child, send)
            }
            CardStyle.FILLED -> Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
            ) { CardBody(widget.child, send) }
        }

        is Widget.Box -> Box(contentAlignment = boxAlignFor(widget.align)) {
            val kids = widget.children
            if (widget.scrim && kids.size > 1) {
                Render(kids.first(), send) // background (e.g. hero image)
                Box(Modifier.matchParentSize().background(Color.Black.copy(alpha = 0.40f)))
                CompositionLocalProvider(LocalContentColor provides Color.White) {
                    Column(modifier = Modifier.fillMaxWidth().padding(16.dp)) {
                        kids.drop(1).forEach { child -> Render(child, send) }
                    }
                }
            } else {
                kids.forEach { child -> Render(child, send) }
            }
        }

        is Widget.Grid -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            widget.children.chunked(2).forEach { rowItems ->
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    rowItems.forEach { item -> Box(modifier = Modifier.weight(1f)) { Render(item, send) } }
                    repeat(2 - rowItems.size) { Spacer(modifier = Modifier.weight(1f)) }
                }
            }
        }

        is Widget.Button -> when (widget.style) {
            ButtonStyle.FILLED -> Button(onClick = { send(widget.onPress) }) { Text(widget.label) }
            ButtonStyle.OUTLINED -> OutlinedButton(onClick = { send(widget.onPress) }) { Text(widget.label) }
            ButtonStyle.TEXT -> TextButton(onClick = { send(widget.onPress) }) { Text(widget.label) }
        }

        is Widget.IconButton -> IconButton(onClick = { send(widget.onPress) }) {
            Icon(
                imageVector = iconFor(widget.icon),
                contentDescription = widget.icon.name.lowercase(),
                tint = iconTintFor(widget.icon),
            )
        }

        is Widget.Chip -> FilterChip(
            selected = widget.selected,
            onClick = { send(widget.onPress) },
            label = { Text(widget.label) },
        )

        is Widget.TextField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { newValue -> send(Event.TextChanged(id = widget.id, value = newValue)) },
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
                onCheckedChange = { newValue -> send(Event.Toggled(id = widget.id, value = newValue)) },
            )
        }

        is Widget.Checkbox -> Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(
                checked = widget.value,
                onCheckedChange = { newValue -> send(Event.Toggled(id = widget.id, value = newValue)) },
            )
            Text(text = widget.label, modifier = Modifier.weight(1f))
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
}

@Composable
private fun iconTintFor(icon: WidgetIcon): Color = when (icon) {
    WidgetIcon.STAR -> MaterialTheme.colorScheme.primary
    else -> LocalContentColor.current
}

/// (background, foreground) for a semantic tone — pulled from the theme so dark
/// mode comes for free.
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

private fun shapeFor(shape: ImageShape): Shape = when (shape) {
    ImageShape.SQUARE -> RectangleShape
    ImageShape.ROUNDED -> RoundedCornerShape(16.dp)
    ImageShape.CIRCLE -> CircleShape
}

private fun ratioFor(ratio: ImageRatio): Float = when (ratio) {
    ImageRatio.WIDE -> 16f / 10f
    ImageRatio.SQUARE -> 1f
    ImageRatio.TALL -> 3f / 4f
}

private fun boxAlignFor(align: BoxAlign): Alignment = when (align) {
    BoxAlign.TOPSTART -> Alignment.TopStart
    BoxAlign.TOPEND -> Alignment.TopEnd
    BoxAlign.CENTER -> Alignment.Center
    BoxAlign.BOTTOMSTART -> Alignment.BottomStart
    BoxAlign.BOTTOMCENTER -> Alignment.BottomCenter
    BoxAlign.BOTTOMEND -> Alignment.BottomEnd
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
        is Widget.Text -> Text(
            text = widget.content,
            style = typographyFor(widget.style),
            color = colorFor(widget.style),
        )
        else -> Render(widget, send)
    }
}
