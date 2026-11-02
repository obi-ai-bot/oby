package com.deskree.oby

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.Image
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import java.time.Instant

/**
 * Settings, opened over Home from your picture: the Electron app's pages on obid, in its order
 * (apps/electron/src/renderer/src/SettingsView.tsx, Sangam's design; the iPhone's SettingsScreen.swift):
 * General, Agents, Remote access, Connectors, About, each a page of his cards. What only the Mac can
 * do (open at login, adding or signing in an agent, connecting GitHub or Tetrix, pairing another
 * device, updating Oby) stays on the Mac, and its rows say so.
 */
@Composable
fun SettingsScreen() {
    val phone = LocalPhone.current
    SettingsPageFrame("Settings") {
        SetGroup(rows = SettingsPage.entries.map { page -> {
            SetRow(page.label, onClick = { phone.push(Route.SettingsPage(page)) }) { Icon(Icons.Filled.ChevronRight, null, tint = Tokens.c.muted) }
        } })
    }
}

/** Electron's Settings pages on obid, in its order. */
enum class SettingsPage(val label: String) { General("General"), Agents("Agents"), Remote("Remote access"), Connectors("Connectors"), About("About") }

@Composable
fun SettingsPageScreen(page: SettingsPage) {
    SettingsPageFrame(page.label) {
        when (page) {
            SettingsPage.General -> { Permissions(); Notifications() }
            SettingsPage.Agents -> AgentsPage()
            SettingsPage.Remote -> RemotePage()
            SettingsPage.Connectors -> ConnectorsPage()
            SettingsPage.About -> SetGroup(rows = listOf { SetRow("Version") { SetValue("${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})") } })
        }
    }
}

/** A Settings page: its 28/600 title over its cards. */
@Composable
private fun SettingsPageFrame(title: String, content: @Composable () -> Unit) {
    Page("") {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = Tokens.gutter).padding(top = 8.dp, bottom = 32.dp)) {
            T(title, 28.sp, Tokens.c.ink, FontWeight.SemiBold, Modifier.padding(bottom = 16.dp).semantics { heading() })
            content()
        }
    }
}

/**
 * Agents (Electron's Settings › Agents): which agent new tasks go to, then a card per agent with its
 * 5-hour session and week, its model and efforts, each changed here as there. Adding, signing in and
 * removing an agent happen on the Mac, where its tool runs (the gateway carries none of those).
 */
@Composable
private fun AgentsPage() {
    val phone = LocalPhone.current
    val scope = rememberCoroutineScope()
    var options by remember { mutableStateOf<ModelOptions?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(phone.base) {
        options = runCatching { phone.runner.modelOptions() }.getOrNull()
        phone.readDefaultAccount()
    }
    val order = listOf("claude", "codex", "cursor")
    val agents = phone.accounts.sortedBy { order.indexOf(it.agent).let { i -> if (i < 0) 9 else i } }
    val ready = agents.filter { it.signedIn }
    val chosen = ready.firstOrNull { it.id == phone.defaultAccount } ?: ready.firstOrNull()
    SetGroup("Default agent", footer = "Add, sign in or remove an agent in Settings › Agents on your Mac, where it runs.", problem = problem, rows = listOf {
        if (chosen == null) SetRow("No agents yet", quiet = true)
        else SetRow("New tasks go to") {
            SetSelect("Default agent", chosen.id, ready.map { it.id to "${it.name} · ${Usage.tool(it.agent) ?: it.agent}" }) { id ->
                if (id != chosen.id) scope.launch { problem = try { phone.setDefaultAccount(id); null } catch (e: Exception) { e.message } }
            }
        }
    })
    for (account in agents) AgentCard(account, options, base(phone.models, account.agent))
}

/** What the tool runs when the account chose nothing (obid's status `models`). */
private fun base(models: ModelSettings?, agent: String): ModelChoice? = when (agent) {
    "claude" -> models?.claude
    "codex" -> models?.codex
    "cursor" -> models?.cursor?.let { ModelChoice(it.model, "") }
    else -> null
}

/** One agent's card: its usage (Electron's: the 5-hour session's bar, the week in words under it), its model and its efforts. */
@Composable
private fun AgentCard(account: Account, options: ModelOptions?, base: ModelChoice?) {
    val phone = LocalPhone.current
    val c = Tokens.c
    val scope = rememberCoroutineScope()
    var problem by remember { mutableStateOf<String?>(null) }
    val usage = AgentUsage(account, rememberNow(60))
    val tool = when (account.agent) { "claude" -> options?.claude; "codex" -> options?.codex; "cursor" -> options?.cursor; else -> null }
    // What the account runs on: its own choice, else the tool's (Electron's `useModels`).
    val model = account.model ?: base?.model ?: ""
    val effort = account.effort ?: base?.effort ?: ""
    val complexEffort = account.complexEffort ?: base?.complexEffort ?: effort
    // The efforts it takes: Codex's are its model's, the others' the tool's.
    val efforts = if (account.agent == "codex") tool?.models?.firstOrNull { it.id == model }?.efforts.orEmpty() else tool?.efforts.orEmpty()
    fun tune(change: Map<String, String>) { scope.launch { problem = try { phone.tune(account.id, change); null } catch (e: Exception) { e.message } } }
    val rows = mutableListOf<@Composable () -> Unit>({
        SetRow(account.name, label = {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(Modifier.width(16.dp).padding(top = 2.dp)) { AgentMark(account.agent, 14.dp) }
                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        T(Usage.tool(account.agent) ?: account.agent, Tokens.textBase, c.ink, modifier = Modifier.weight(1f))
                        usage.session?.let { SetValue("$it% used") }
                    }
                    when {
                        !account.signedIn -> SetAbout(account.label ?: "Not signed in: sign in on your Mac")
                        usage.session != null -> {
                            SetAbout("5-hour session" + (usage.sessionResets?.let { " · resets $it" } ?: ""))
                            SetMeter(usage.session.toDouble(), Modifier.padding(vertical = 4.dp))
                            SetAbout(usage.week)
                        }
                        else -> SetAbout(usage.week)
                    }
                }
            }
        })
    }, {
        SetRow("Model") {
            ModelMenu("${account.name} model and effort", choices(tool?.models.orEmpty().map { it.id to it.name }, model), model,
                      if (account.agent == "cursor") emptyList() else choices(efforts.map { it to effortName(it) }, effort), effort,
                      onModel = { if (it != model) tune(mapOf("model" to it)) }, onEffort = { if (it != effort) tune(mapOf("effort" to it)) })
        }
    })
    if (account.agent != "cursor") rows += {
        SetRow("Effort on complex tasks") {
            SetSelect("${account.name} complex task effort", complexEffort, choices(tool?.complexEfforts.orEmpty().map { it to effortName(it) }, complexEffort)) {
                if (it != complexEffort) tune(mapOf("complexEffort" to it))
            }
        }
    }
    SetGroup(account.name, problem = problem, rows = rows)
}

private val effortNames = mapOf("low" to "Low", "medium" to "Medium", "high" to "High", "xhigh" to "Extra high", "max" to "Max", "ultra" to "Ultra")
private fun effortName(effort: String) = effortNames[effort] ?: effort

/** A pop-up's choices, the one set kept even when the choices don't have it (Electron's `choices`). */
internal fun choices(list: List<Pair<String, String>>, set: String) = (if (set.isEmpty() || list.any { it.first == set }) emptyList() else listOf(set to set)) + list

/** An account's usage as Electron's `accountRow` and `weekWords` say it (the iPhone's AgentUsage). */
class AgentUsage(a: Account, now: Instant) {
    /** The 5-hour session's share: signed in, not Cursor; past its reset, 0. */
    val session: Int?
    val sessionResets: String?
    val week: String

    init {
        val over = a.fiveHourResets?.let { it <= now } ?: false
        session = if (a.agent == "cursor" || !a.signedIn || a.fiveHour == null) null else if (over) 0 else Math.round(a.fiveHour * 100).toInt()
        sessionResets = if (session != null && !over) a.fiveHourResets?.let(Format::time) else null
        val used = if (a.agent == "cursor") null else (a.sevenDay ?: a.fiveHour)?.let { Math.round(it * 100).toInt() }
        val resets = when {
            !a.signedIn -> a.label ?: "Not signed in"
            a.agent == "cursor" -> "Cursor doesn’t report usage"
            used == null -> "No use reported yet"
            else -> a.sevenDayResets?.let { "Resets ${Format.weekday(it)}" } ?: "No reset reported"
        }
        week = when {
            a.sevenDay != null && used != null -> "This week $used% · " + if (resets.startsWith("Resets")) "resets" + resets.drop(6) else resets
            a.sevenDay == null && session != null -> "No weekly use reported yet"
            else -> resets
        }
    }
}

/**
 * Electron's ModelPicker: one button, the model and after it in grey its effort; one menu with the
 * models, then Effort and the efforts. No efforts (Cursor's): the models only.
 */
@Composable
private fun ModelMenu(title: String, models: List<Pair<String, String>>, model: String, efforts: List<Pair<String, String>>, effort: String,
                      onModel: (String) -> Unit, onEffort: (String) -> Unit) {
    val c = Tokens.c
    var open by remember { mutableStateOf(false) }
    fun name(id: String, list: List<Pair<String, String>>) = list.firstOrNull { it.first == id }?.second ?: id.ifEmpty { "—" }
    Box {
        Row(Modifier.height(32.dp).clip(RoundedCornerShape(Tokens.iconRadius)).background(c.field).border(1.dp, c.line, RoundedCornerShape(Tokens.iconRadius))
            .clickable { open = true }.padding(start = 14.dp, end = 10.dp)
            .semantics { contentDescription = "$title: " + (if (efforts.isEmpty()) name(model, models) else "${name(model, models)}, ${name(effort, efforts)}") },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            T(name(model, models), Tokens.textBase, c.ink, lines = 1, modifier = Modifier.widthIn(max = 120.dp))
            if (efforts.isNotEmpty()) T(name(effort, efforts), Tokens.textBase, c.muted, lines = 1)
            Icon(Icons.Filled.ExpandMore, null, Modifier.size(14.dp), tint = c.muted)
        }
        DropdownMenu(open, { open = false }, containerColor = c.surface) {
            for ((id, label) in models) DropdownMenuItem({ T(label, Tokens.textBase, c.ink) }, { open = false; onModel(id) },
                                                         trailingIcon = { if (id == model) Icon(Icons.Filled.Check, null, tint = c.ink) })
            if (efforts.isNotEmpty()) {
                HorizontalDivider(color = c.line)
                T("Effort", Tokens.textSm, c.muted, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
                for ((id, label) in efforts) DropdownMenuItem({ T(label, Tokens.textBase, c.ink) }, { open = false; onEffort(id) },
                                                              trailingIcon = { if (id == effort) Icon(Icons.Filled.Check, null, tint = c.ink) })
            }
        }
    }
}

/** Remote access, from the phone's side: the Mac it reaches, how, until when, and Unpair. Pairing a device and the list of paired ones are the Mac's own. */
@Composable
private fun RemotePage() {
    val phone = LocalPhone.current
    val c = Tokens.c
    var unpairing by remember { mutableStateOf(false) }
    val link = phone.link
    if (link == null) {
        SetGroup("Your Mac", footer = "On your Mac, open Settings › Remote access › Pair a device, then scan its code.", rows = listOf { SetRow("No Mac paired yet", quiet = true) })
        return
    }
    SetGroup("Your Mac", footer = "Unpair makes this phone forget your Mac. To end its pairing on the Mac too, revoke it in your Mac’s Settings › Remote access.", rows = listOf(
        { SetRow(link.name, about = listOfNotNull(phone.route?.label, phone.pairedUntil?.let { "Paired until ${Format.monthDay(it)}" }).joinToString(" · ")) {
            SetValue(phone.notice?.title ?: if (phone.connection == Connection.Live) "Connected" else "Connecting…", phone.connection == Connection.Live)
        } },
        { SetRow("Unpair this phone") { SetButton("Unpair", destructive = true) { unpairing = true } } },
    ))
    if (unpairing) AlertDialog(onDismissRequest = { unpairing = false }, containerColor = c.surface,
        title = { T("Unpair from ${link.name}?", 17.sp, c.ink, FontWeight.SemiBold) },
        text = { T("This phone forgets its device token. Scan the code on your Mac to connect again.", Tokens.textBase, c.text) },
        confirmButton = { TextButton({ unpairing = false; phone.unpair(); phone.homePath = emptyList() }) { T("Unpair", Tokens.textBase, c.danger) } },
        dismissButton = { TextButton({ unpairing = false }) { T("Cancel", Tokens.textBase, c.muted) } })
}

/** Connectors (Electron's on obid): Connected, then Available, each with its line. Connecting and disconnecting happen on the Mac, whose logins they are. */
@Composable
private fun ConnectorsPage() {
    val phone = LocalPhone.current
    class Connector(val name: String, val mark: Int, val tinted: Boolean, val about: String, val connected: Boolean)
    // GitHub: obid's gh is signed in (its GitHub work says so, and as whom); Tetrix: the accounts' agents have it.
    val github = phone.gitHubWork
    val tetrix = phone.accounts.any { it.tetrix == true }
    val all = listOf(
        Connector("GitHub", R.drawable.mark_github, true, if (github?.signedIn == true) github.login ?: "Signed in" else "Repos, pull requests and checks", github?.signedIn == true),
        Connector("Tetrix", R.drawable.mark_tetrix, false, if (tetrix) "For all your agents" else "Project knowledge and blast radius", tetrix),
    )
    for (connected in listOf(true, false)) {
        val items = all.filter { it.connected == connected }
        if (items.isEmpty()) continue
        SetGroup(if (connected) "Connected" else "Available",
                 footer = if (connected) null else "Connect them in Settings › Connectors on your Mac: their logins stay there, never on this phone.",
                 rows = items.map { item -> {
                     SetRow(item.name, label = {
                         Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                             Image(painterResource(item.mark), null, Modifier.size(22.dp), colorFilter = if (item.tinted) ColorFilter.tint(Tokens.c.ink) else null)
                             Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                 T(item.name, Tokens.textBase, Tokens.c.ink)
                                 SetAbout(item.about)
                             }
                         }
                     }) { SetValue(if (item.connected) "Connected" else "On your Mac", item.connected) }
                 } })
    }
}

/** General › Permissions on your Mac: the mode new tasks get, and the rules Always allow (or Deny) saved, each removed with its button. */
@Composable
private fun Permissions() {
    val phone = LocalPhone.current
    val scope = rememberCoroutineScope()
    var rules by remember { mutableStateOf<List<RunnerRule>?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(phone.base) {
        rules = try { phone.runner.permissionRules() } catch (e: RunnerError) { if (e.status != 404) problem = e.message; null } catch (e: Exception) { problem = e.message; null }
    }
    val mode = phone.defaultMode
    val rows = mutableListOf<@Composable () -> Unit>()
    if (mode == null && rules == null) rows += { SetRow("This Oby doesn't ask before agents act.", quiet = true) }
    else {
        rows += { SetRow("Default mode", about = mode?.help) {
            SetSelect("Default mode", mode, RunnerMode.offered(mode).map { it to it.label }) { m ->
                if (m != null) scope.launch { problem = try { phone.setDefaultMode(m); null } catch (e: Exception) { e.message } }
            }
        } }
        // None saved: no row, as Electron's (its note says how one gets here).
        for (rule in rules.orEmpty()) rows += {
            SetRow(rule.rule, about = listOfNotNull(if (rule.decision == "deny") "Denied" else "Allowed", rule.repo).joinToString(" · "), label = { SetCode(rule.rule) }) {
                SetButton("Remove", destructive = true) {
                    scope.launch { problem = try { phone.runner.removeRule(rule.id); rules = rules?.filter { it.id != rule.id }; null } catch (e: Exception) { e.message } }
                }
            }
        }
    }
    SetGroup("Permissions on your Mac", problem = problem, rows = rows,
             footer = "Default mode sets how often your agents check with you before they edit files or run commands. To change it for one task, use that task’s Mode menu. Always allow adds a rule here; remove it to be asked again.")
}

/** General › Notifications: whether Oby may alert you when a task needs you, a way to allow it, and the way to the system's settings. */
@Composable
private fun Notifications() {
    val phone = LocalPhone.current
    val activity = LocalActivity.current
    var allowed by remember { mutableStateOf(Notifications.allowed(activity)) }
    val lifecycle = androidx.lifecycle.compose.LocalLifecycleOwner.current
    androidx.compose.runtime.DisposableEffect(lifecycle) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, e -> if (e == androidx.lifecycle.Lifecycle.Event.ON_RESUME) allowed = Notifications.allowed(activity) }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer) }
    }
    val configured = Notifications.configured(activity)
    val pushes = phone.pushes
    SetGroup("Notifications", rows = listOf(
        { SetRow("Alerts") {
            SetValue(if (allowed) "Allowed" else "Off")
            SetButton(if (allowed) "Open Settings" else "Allow Alerts") { if (allowed) activity.openNotificationSettings() else { Store.prefs(activity).edit().remove("notifications.asked").apply(); activity.askNotifications() } }
        } },
        { SetRow("Push from your Mac", about = when {
            !configured -> "This build has no Firebase project yet, so alerts come only while Oby is open."
            pushes == true -> "Your Mac sends alerts to this phone."
            pushes == false -> "Oby on your Mac doesn't send pushes yet."
            else -> null
        }) { SetValue(if (configured && pushes == true) "On" else "Off") } },
    ), footer = "Oby alerts you when a job needs you, like a question or a failed run. Tap an alert to open the job.")
}
