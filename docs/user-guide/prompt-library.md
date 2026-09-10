# Prompts Library

The Prompts Library stores and manages all your prompt templates — both the 24 built-in AI automation prompts and your custom templates. Prompts are injected directly into the active agent terminal or run headless for quick one-shot operations.

> **Looking for one-click AI automation?** See [Smart Prompts](smart-prompts.md) for the full guide on built-in automation prompts, context variables, and headless execution.

## Opening the Drawer

- **Cmd+Shift+K** — Toggle the prompt library drawer
- **Toolbar button** — Prompt library icon in the main toolbar

## Browsing and Searching

When the drawer opens, the search input is focused automatically. Type to filter prompts by name, description, or content. Matching is case-insensitive and searches all three fields simultaneously.

Use the category tabs to narrow the list:

| Tab | Shows |
|-----|-------|
| **All** | Every saved prompt, sorted by most recently used |
| **Custom** | User-created prompts |
| **Favorites** | Prompts you have starred |
| **Recent** | Last 10 prompts you used |

## Keyboard Navigation

The search field always keeps keyboard focus while the drawer is open — `Tab`
never leaves it.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move selection up/down |
| `Enter` | Run the selected prompt using its Auto-execute setting |
| Double-click | Insert and submit exactly once |
| `Tab` / `Shift+Tab` | Cycle the category chips (All → Custom → Recent → Favorites), wrapping at either end, without leaving the search field |
| `Ctrl+N` / `Cmd+N` | Create a new prompt |
| `Ctrl+E` / `Cmd+E` | Edit the selected prompt |
| `Ctrl+F` / `Cmd+F` | Toggle favorite on the selected prompt |
| `Escape` | Close the drawer |

## Creating a Prompt

1. Open the drawer (`Cmd+Shift+K`) and click **+ New Prompt**, or press `Ctrl+N`/`Cmd+N`
2. Fill in the fields:
   - **Name** (required) — shown in the list
   - **Description** — optional subtitle, also searchable
   - **Content** (required) — the text to insert; use `{{variable}}` for dynamic values
   - **Auto-execute** — submit immediately, or leave the inserted text editable for review
   - **Keyboard Shortcut** — optional global shortcut to trigger this prompt directly
3. Click **Save**

## Editing and Deleting

- Click the **pencil icon** on any prompt row, or select it and press `Ctrl+E`/`Cmd+E`
- Click the **trash icon** to delete — a confirmation dialog appears before deletion

## Import & Export

The library backs both this drawer and the **Settings > Smart Prompts** screen, so exporting
or importing prompts there covers everything saved here too. See [Smart
Prompts](smart-prompts.md#import--export) for scopes (everything / modified only / custom
only) and how conflicts are reviewed on import.

## Variable Substitution

Use `{{variable_name}}` placeholders in prompt content. When you send a prompt that contains variables, a dialog appears asking you to fill in each value before injection.

```
cd {{project_dir}} && cargo test -- {{test_filter}}
```

### Built-in Variables

These are resolved automatically by the backend when present:

| Variable | Value |
|----------|-------|
| `{{diff}}` | Current git diff |
| `{{changed_files}}` | List of changed files |
| `{{repo_name}}` | Repository name |
| `{{branch}}` | Current branch name |
| `{{cwd}}` | Current working directory |

### Custom Variables

Any `{{name}}` not in the built-in list becomes a custom input field in the variable dialog. You can optionally add a description and default value per variable when editing the prompt — the description appears as placeholder text in the dialog.

### Inserting with Variables

The variable dialog offers two actions:

- **Insert** — writes the resolved text to the terminal input line (you can review before pressing Enter)
- **Insert & Run** — submits the resolved text once through the agent-aware command path

These explicit actions override the saved Auto-execute setting for that use.

## Favorites and Pinning

Click the **star icon** on any prompt row to toggle its favorite status. Favorited prompts appear at the top of any list view with a `★` prefix and are accessible via the **Favorites** category tab.

## Recently Used

The **Recent** tab shows the last 10 prompts you sent, in order of use. Recency is also used to sort the **All** view — most recently used prompts appear first.

## Sending to Terminal or Compose

Selecting a prompt (click or Enter) writes its content either to the **Compose
box** or directly into the **terminal input**, depending on the prompt's
**Target** setting (edit the prompt to change it):

- **Auto** (default) — fills the Compose box if it's already open; otherwise
  goes straight to the terminal input. This avoids popping Compose open just
  to hold text you didn't ask to review.
- **Compose box** — always opens Compose and fills it, regardless of whether
  it was already open.
- **Terminal** — always writes to the terminal input.

Auto-execute submits the result immediately once it reaches its destination;
otherwise the text remains editable. If the prompt has variables, the
variable dialog appears first and its **Insert** or **Insert & Run** choice
takes precedence over the saved Auto-execute setting.

Shell script, Headless, and API prompts ignore Target/Auto-execute entirely —
selecting one from the drawer runs it the same way it would run from the
toolbar or Command Palette (see [Execution
Modes](smart-prompts.md#execution-modes)).

The drawer closes automatically after a successful injection or execution,
and focus returns to the terminal.

---

## Run Commands

A lighter-weight alternative for per-branch one-off commands:

- **Cmd+R** — Run the saved command for the active branch
- **Cmd+Shift+R** — Edit the command before running

Configure run commands in **Settings → Repository → Scripts** tab.
