<script lang="ts">
  /**
   * The macro bar's editor. Three kinds of macro exist and no more — emote,
   * chat line, panel — so this form has no way to express a combat action
   * (IMP-4.6).
   */
  import {
    loadMacros,
    MACRO_COUNT,
    MACRO_PANELS,
    assignMacro,
    clearMacro,
    isSayable,
    macroPanelVisible,
    macros,
    runMacro,
    type MacroAction,
    type MacroPanelId,
  } from '../stores/macroStore'
  import { SLASH_EMOTE_ANIMS } from '../stores/emoteStore'

  const emotes = [...SLASH_EMOTE_ANIMS].sort()
  const panels = Object.keys(MACRO_PANELS) as MacroPanelId[]

  interface Props {
    /** Active character id — macros are saved per character. */
    characterId: number | null
  }

  let { characterId }: Props = $props()

  $effect(() => {
    if (characterId != null) loadMacros(characterId)
  })

  let editing = $state<number | null>(null)
  let kind = $state<MacroAction['kind']>('emote')
  let emote = $state(emotes[0])
  let message = $state('')
  let panel = $state<MacroPanelId>(panels[0])

  function open(index: number) {
    editing = index
    const current = $macros[index]
    kind = current?.kind ?? 'emote'
    emote = current?.kind === 'emote' ? current.emote : emotes[0]
    message = current?.kind === 'say' ? current.message : ''
    panel = current?.kind === 'panel' ? current.panel : panels[0]
  }

  function save() {
    if (editing === null) return
    if (kind === 'emote') assignMacro(editing, { kind: 'emote', emote })
    else if (kind === 'say') assignMacro(editing, { kind: 'say', message })
    else assignMacro(editing, { kind: 'panel', panel })
    editing = null
  }

  function describe(action: MacroAction | null): string {
    if (!action) return 'empty'
    if (action.kind === 'emote') return `/emote ${action.emote}`
    if (action.kind === 'say') return action.message
    return `open ${action.panel}`
  }
</script>

{#if $macroPanelVisible}
  <div class="macro-panel" aria-label="Macros">
    <div class="panel-header">
      <span class="panel-title">Macros</span>
      <span class="hint">ALT+1..0</span>
      <button
        class="close-btn"
        title="Close"
        onclick={() => macroPanelVisible.set(false)}>×</button
      >
    </div>

    <div class="rows">
      {#each { length: MACRO_COUNT } as _, i (i)}
        <div class="row">
          <span class="key">{(i + 1) % 10}</span>
          <button class="slot" onclick={() => runMacro($macros[i])}>
            {describe($macros[i])}
          </button>
          <button class="edit" title="Edit" onclick={() => open(i)}>✎</button>
          <button class="edit" title="Clear" onclick={() => clearMacro(i)}
            >×</button
          >
        </div>
      {/each}
    </div>

    {#if editing !== null}
      <div class="editor">
        <div class="kinds">
          {#each ['emote', 'say', 'panel'] as const as k (k)}
            <label>
              <input type="radio" bind:group={kind} value={k} />{k}
            </label>
          {/each}
        </div>
        {#if kind === 'emote'}
          <select bind:value={emote}>
            {#each emotes as name (name)}<option value={name}>{name}</option
              >{/each}
          </select>
        {:else if kind === 'say'}
          <input
            type="text"
            maxlength="120"
            bind:value={message}
            placeholder="a line of chat (no / commands)"
          />
        {:else}
          <select bind:value={panel}>
            {#each panels as name (name)}<option value={name}>{name}</option
              >{/each}
          </select>
        {/if}
        <div class="actions">
          <button
            onclick={save}
            disabled={kind === 'say' && !isSayable(message)}>Save</button
          >
          <button onclick={() => (editing = null)}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .macro-panel {
    position: fixed;
    left: 16px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    width: 260px;
    max-height: 70vh;
    overflow-y: auto;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    pointer-events: auto;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }

  .panel-title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
    color: #8fe08f;
  }

  .hint {
    color: #7f8f9f;
    font-size: 10px;
  }

  .close-btn,
  .edit {
    background: none;
    border: none;
    color: #b0bcc8;
    cursor: pointer;
    font-size: 13px;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .key {
    width: 14px;
    color: #7f8f9f;
  }

  .slot {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    color: inherit;
    font: inherit;
    padding: 3px 5px;
    cursor: pointer;
  }

  .editor {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .kinds {
    display: flex;
    gap: 8px;
  }

  .actions {
    display: flex;
    gap: 6px;
  }

  .editor input[type='text'],
  .editor select {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    color: inherit;
    font: inherit;
    padding: 3px 5px;
  }
</style>
