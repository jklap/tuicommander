import {
  Component,
  For,
  Show,
  createMemo,
  createResource,
  createSignal,
} from "solid-js";
import { invoke } from "../../../invoke";
import { providerRegistryStore, type ProviderEntry, type ProviderType, type ModelEntry, type SlotName } from "../../../stores/providerRegistry";
import s from "../Settings.module.css";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PROVIDER_TYPES: { value: ProviderType; label: string; comingSoon?: boolean }[] = [
  { value: "anthropic",   label: "Anthropic" },
  { value: "open_ai",     label: "OpenAI" },
  { value: "gemini",      label: "Google Gemini" },
  { value: "deep_seek",   label: "DeepSeek" },
  { value: "mistral",     label: "Mistral" },
  { value: "fireworks",   label: "Fireworks AI" },
  { value: "samba_nova",  label: "SambaNova" },
  { value: "moonshot",    label: "Moonshot" },
  { value: "xai",         label: "xAI (Grok)" },
  { value: "zai",         label: "Zhipu AI" },
  { value: "open_router", label: "OpenRouter" },
  { value: "requesty",    label: "Requesty" },
  { value: "lite_llm",    label: "LiteLLM" },
  { value: "ollama",      label: "Ollama (local)" },
  { value: "lm_studio",   label: "LM Studio (local)" },
  { value: "bedrock",     label: "AWS Bedrock", comingSoon: true },
  { value: "vertex",      label: "Google Vertex", comingSoon: true },
  { value: "custom",      label: "Custom (OpenAI-compatible)" },
];

const SLOT_LABELS: Record<SlotName, string> = {
  chat:          "Chat",
  agent_default: "Agent (default)",
  agent_search:  "Agent search",
  agent_read:    "Agent read",
  agent_write:   "Agent write",
  headless:      "Headless / Smart Prompts",
  enrichment:    "Enrichment",
};

const SLOT_NAMES: SlotName[] = [
  "chat", "agent_default", "agent_search", "agent_read", "agent_write",
  "headless", "enrichment",
];

const AGENT_PHASE_SLOTS: SlotName[] = ["agent_search", "agent_read", "agent_write"];

const LOCAL_PROVIDER_TYPES: ProviderType[] = ["ollama", "lm_studio", "lite_llm"];

function needsApiKey(type: ProviderType): boolean {
  return !LOCAL_PROVIDER_TYPES.includes(type);
}

// ---------------------------------------------------------------------------
// Add Provider Wizard
// ---------------------------------------------------------------------------

const AddProviderForm: Component<{ onAdd: (e: ProviderEntry) => void; onCancel: () => void }> = (props) => {
  const [type, setType] = createSignal<ProviderType>("anthropic");
  const [label, setLabel] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");

  const selectedDef = createMemo(() => PROVIDER_TYPES.find((p) => p.value === type()));

  async function submit() {
    if (!label().trim()) { setError("Label is required"); return; }
    if (selectedDef()?.comingSoon) { setError("This provider is not yet supported"); return; }
    if (needsApiKey(type()) && !apiKey().trim()) { setError("API key is required"); return; }
    setSaving(true);
    setError("");
    try {
      const id = `${type()}-${Date.now()}`;
      const entry: ProviderEntry = {
        id,
        type: type(),
        label: label().trim(),
        base_url: baseUrl().trim() || null,
      };
      if (needsApiKey(type()) && apiKey().trim()) {
        await providerRegistryStore.saveKey(id, apiKey().trim());
      }
      props.onAdd(entry);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div class={s.section} data-testid="add-provider-form">
      <h3>Add Provider</h3>
      <div class={s.group}>
        <label>Type</label>
        <select value={type()} onChange={(e) => setType(e.currentTarget.value as ProviderType)}>
          <For each={PROVIDER_TYPES}>
            {(p) => (
              <option value={p.value}>
                {p.label}{p.comingSoon ? " (coming soon)" : ""}
              </option>
            )}
          </For>
        </select>
      </div>
      <Show when={selectedDef()?.comingSoon}>
        <p style={{ color: "var(--color-fg-muted)", "font-size": "0.85em" }}>
          Bedrock and Vertex require additional SDK integration — coming soon.
        </p>
      </Show>
      <div class={s.group}>
        <label>Label</label>
        <input
          type="text"
          placeholder="e.g. Anthropic (personal)"
          value={label()}
          onInput={(e) => setLabel(e.currentTarget.value)}
        />
      </div>
      <Show when={type() !== "anthropic" && type() !== "open_ai" && type() !== "gemini"}>
        <div class={s.group}>
          <label>Base URL (optional)</label>
          <input
            type="text"
            placeholder="Leave blank to use default"
            value={baseUrl()}
            onInput={(e) => setBaseUrl(e.currentTarget.value)}
          />
        </div>
      </Show>
      <Show when={needsApiKey(type())}>
        <div class={s.group}>
          <label>API Key</label>
          <input
            type="password"
            placeholder="sk-…"
            value={apiKey()}
            onInput={(e) => setApiKey(e.currentTarget.value)}
          />
        </div>
      </Show>
      <Show when={error()}>
        <p style={{ color: "var(--color-danger)" }}>{error()}</p>
      </Show>
      <div style={{ display: "flex", gap: "8px", "margin-top": "8px" }}>
        <button onClick={submit} disabled={saving()}>
          {saving() ? "Adding…" : "Add"}
        </button>
        <button onClick={props.onCancel}>Cancel</button>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Add Model Form
// ---------------------------------------------------------------------------

const AddModelForm: Component<{
  providerId: string;
  onAdd: (m: ModelEntry) => void;
  onCancel: () => void;
}> = (props) => {
  const [modelName, setModelName] = createSignal("");
  const [tier, setTier] = createSignal<ModelEntry["tier"]>("standard");
  const [error, setError] = createSignal("");

  function submit() {
    if (!modelName().trim()) { setError("Model name is required"); return; }
    const id = `model-${props.providerId}-${modelName().trim().replace(/[^a-z0-9]/gi, "-")}`;
    props.onAdd({ id, provider_id: props.providerId, model_name: modelName().trim(), tier: tier() });
  }

  return (
    <div data-testid="add-model-form" style={{ "margin-top": "8px" }}>
      <div class={s.group}>
        <label>Model name</label>
        <input
          type="text"
          placeholder="e.g. claude-sonnet-4-5-20241022"
          value={modelName()}
          onInput={(e) => setModelName(e.currentTarget.value)}
        />
      </div>
      <div class={s.group}>
        <label>Tier</label>
        <select value={tier()} onChange={(e) => setTier(e.currentTarget.value as ModelEntry["tier"])}>
          <option value="economic">Economic</option>
          <option value="standard">Standard</option>
          <option value="premium">Premium</option>
        </select>
      </div>
      <Show when={error()}>
        <p style={{ color: "var(--color-danger)" }}>{error()}</p>
      </Show>
      <div style={{ display: "flex", gap: "8px" }}>
        <button onClick={submit}>Add model</button>
        <button onClick={props.onCancel}>Cancel</button>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Provider Card
// ---------------------------------------------------------------------------

const ProviderCard: Component<{ provider: ProviderEntry }> = (props) => {
  const [showAddModel, setShowAddModel] = createSignal(false);
  const [keyInput, setKeyInput] = createSignal("");
  const [savingKey, setSavingKey] = createSignal(false);
  const [keyMsg, setKeyMsg] = createSignal("");

  const models = createMemo(() =>
    providerRegistryStore.state.registry.models.filter(
      (m) => m.provider_id === props.provider.id
    )
  );

  const hasKey = createMemo(() =>
    providerRegistryStore.state.keyStatus[props.provider.id] ?? false
  );

  const isOllama = () => props.provider.type === "ollama";

  const [ollamaModels] = createResource(
    () => isOllama() ? props.provider.id : null,
    async (providerId) => {
      try {
        const result = await invoke<{ available: boolean; models: string[] }>(
          "check_ollama_models",
          { providerId }
        );
        return result.models ?? [];
      } catch {
        return [];
      }
    }
  );

  async function saveKey() {
    if (!keyInput().trim()) return;
    setSavingKey(true);
    setKeyMsg("");
    try {
      await providerRegistryStore.saveKey(props.provider.id, keyInput().trim());
      setKeyInput("");
      setKeyMsg("Key saved");
    } catch (e) {
      setKeyMsg(`Error: ${String(e)}`);
    } finally {
      setSavingKey(false);
    }
  }

  async function deleteKey() {
    setSavingKey(true);
    try {
      await providerRegistryStore.deleteKey(props.provider.id);
      setKeyMsg("Key removed");
    } finally {
      setSavingKey(false);
    }
  }

  return (
    <div
      data-testid={`provider-card-${props.provider.id}`}
      style={{
        border: "1px solid var(--color-border)",
        "border-radius": "6px",
        padding: "12px",
        "margin-bottom": "8px",
      }}
    >
      <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
        <div>
          <strong>{props.provider.label}</strong>
          {" "}
          <span style={{ color: "var(--color-fg-muted)", "font-size": "0.8em" }}>
            ({PROVIDER_TYPES.find((p) => p.value === props.provider.type)?.label ?? props.provider.type})
          </span>
          {" "}
          <span
            style={{ color: hasKey() ? "var(--color-success)" : "var(--color-fg-muted)", "font-size": "0.8em" }}
            data-testid={`key-status-${props.provider.id}`}
          >
            {needsApiKey(props.provider.type)
              ? hasKey() ? "✓ key" : "no key"
              : "no key needed"}
          </span>
        </div>
        <button
          data-testid={`remove-provider-${props.provider.id}`}
          onClick={() => providerRegistryStore.removeProvider(props.provider.id)}
          title="Remove provider"
          style={{ color: "var(--color-danger)" }}
        >
          ×
        </button>
      </div>

      {/* Models */}
      <div style={{ "margin-top": "8px" }}>
        <div style={{ "font-size": "0.85em", color: "var(--color-fg-muted)" }}>
          Models ({models().length}):
        </div>
        <For each={models()}>
          {(model) => (
            <div
              data-testid={`model-entry-${model.id}`}
              style={{ display: "flex", "align-items": "center", gap: "8px", "margin-top": "4px" }}
            >
              <span>{model.model_name}</span>
              <span style={{ "font-size": "0.8em", color: "var(--color-fg-muted)" }}>({model.tier})</span>
              <button
                data-testid={`remove-model-${model.id}`}
                onClick={() => providerRegistryStore.removeModel(model.id)}
                title="Remove model"
                style={{ color: "var(--color-danger)", "font-size": "0.8em" }}
              >
                ×
              </button>
            </div>
          )}
        </For>

        {/* Ollama live discovery */}
        <Show when={isOllama() && (ollamaModels()?.length ?? 0) > 0}>
          <div style={{ "margin-top": "4px", "font-size": "0.8em", color: "var(--color-fg-muted)" }}>
            Available: {ollamaModels()!.join(", ")}
          </div>
        </Show>

        <Show
          when={showAddModel()}
          fallback={
            <button
              data-testid={`add-model-btn-${props.provider.id}`}
              onClick={() => setShowAddModel(true)}
              style={{ "margin-top": "4px", "font-size": "0.85em" }}
            >
              + Add model
            </button>
          }
        >
          <AddModelForm
            providerId={props.provider.id}
            onAdd={(m) => {
              providerRegistryStore.addModel(m);
              setShowAddModel(false);
            }}
            onCancel={() => setShowAddModel(false)}
          />
        </Show>
      </div>

      {/* API Key management */}
      <Show when={needsApiKey(props.provider.type)}>
        <div style={{ "margin-top": "8px" }}>
          <div class={s.group} style={{ display: "flex", gap: "8px", "align-items": "center" }}>
            <input
              type="password"
              placeholder={hasKey() ? "Replace existing key…" : "Enter API key…"}
              value={keyInput()}
              onInput={(e) => setKeyInput(e.currentTarget.value)}
              style={{ flex: "1" }}
              data-testid={`key-input-${props.provider.id}`}
            />
            <button onClick={saveKey} disabled={savingKey() || !keyInput().trim()}>
              Save
            </button>
            <Show when={hasKey()}>
              <button onClick={deleteKey} disabled={savingKey()} style={{ color: "var(--color-danger)" }}>
                Remove
              </button>
            </Show>
          </div>
          <Show when={keyMsg()}>
            <div style={{ "font-size": "0.8em", color: "var(--color-fg-muted)" }}>{keyMsg()}</div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Slot Assignments
// ---------------------------------------------------------------------------

const SlotAssignments: Component = () => {
  const allModels = () => providerRegistryStore.state.registry.models;

  function modelLabel(modelId: string): string {
    const model = allModels().find((m) => m.id === modelId);
    if (!model) return modelId;
    const provider = providerRegistryStore.state.registry.providers.find(
      (p) => p.id === model.provider_id
    );
    return provider ? `${model.model_name} (${provider.label})` : model.model_name;
  }

  const [testResults, setTestResults] = createSignal<Record<string, string>>({});

  async function testSlot(slot: SlotName) {
    setTestResults((r) => ({ ...r, [slot]: "Testing…" }));
    try {
      const result = await invoke<string>("test_slot_connection", { slot });
      setTestResults((r) => ({ ...r, [slot]: result }));
    } catch (e) {
      setTestResults((r) => ({ ...r, [slot]: `Error: ${String(e)}` }));
    }
  }

  return (
    <div class={s.section} data-testid="slot-assignments">
      <h3>Slot Assignments</h3>
      <For each={SLOT_NAMES}>
        {(slot) => {
          const currentModelId = () => providerRegistryStore.state.registry.slots[slot];
          const isAgentPhase = AGENT_PHASE_SLOTS.includes(slot);
          const fallbackHint = isAgentPhase && !currentModelId();

          return (
            <div class={s.group} data-testid={`slot-row-${slot}`}>
              <label>
                {SLOT_LABELS[slot]}
                <Show when={fallbackHint}>
                  {" "}
                  <span style={{ "font-size": "0.8em", color: "var(--color-fg-muted)" }}>
                    (falls back to agent default)
                  </span>
                </Show>
              </label>
              <div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
                <select
                  data-testid={`slot-select-${slot}`}
                  value={currentModelId() ?? ""}
                  onChange={(e) => {
                    const v = e.currentTarget.value;
                    if (v) providerRegistryStore.setSlot(slot, v);
                    else providerRegistryStore.clearSlot(slot);
                  }}
                >
                  <option value="">— unset —</option>
                  <For each={allModels()}>
                    {(model) => (
                      <option value={model.id}>{modelLabel(model.id)}</option>
                    )}
                  </For>
                </select>
                <Show when={currentModelId()}>
                  <button
                    data-testid={`test-slot-${slot}`}
                    onClick={() => testSlot(slot)}
                    title="Test connection"
                  >
                    Test
                  </button>
                </Show>
              </div>
              <Show when={testResults()[slot]}>
                <div style={{ "font-size": "0.8em", color: "var(--color-fg-muted)" }}>
                  {testResults()[slot]}
                </div>
              </Show>
            </div>
          );
        }}
      </For>
    </div>
  );
};

// ---------------------------------------------------------------------------
// ProvidersTab
// ---------------------------------------------------------------------------

export const ProvidersTab: Component = () => {
  const [showAddForm, setShowAddForm] = createSignal(false);

  const providers = () => providerRegistryStore.state.registry.providers;

  return (
    <div data-testid="providers-tab">
      <div class={s.section}>
        <h3>
          Providers
          <Show when={!showAddForm()}>
            {" "}
            <button
              data-testid="add-provider-btn"
              onClick={() => setShowAddForm(true)}
              style={{ "font-size": "0.85em", "margin-left": "8px" }}
            >
              + Add
            </button>
          </Show>
        </h3>

        <Show when={showAddForm()}>
          <AddProviderForm
            onAdd={(entry) => {
              providerRegistryStore.addProvider(entry);
              setShowAddForm(false);
            }}
            onCancel={() => setShowAddForm(false)}
          />
        </Show>

        <Show
          when={providers().length > 0}
          fallback={
            <p style={{ color: "var(--color-fg-muted)" }}>
              No providers configured. Add one to get started.
            </p>
          }
        >
          <For each={providers()}>
            {(provider) => <ProviderCard provider={provider} />}
          </For>
        </Show>
      </div>

      <SlotAssignments />
    </div>
  );
};
