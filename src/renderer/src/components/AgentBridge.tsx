import { Bot, Check, Copy, ExternalLink, ShieldCheck, Terminal } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import type { McpConfig } from '@shared/types'
import { api, isDemo } from '../lib/bridge'
import { useStudio } from '../store'
import { Badge, Button, EmptyState } from './primitives'

const TOOLS: Array<{ name: string; description: string }> = [
  { name: 'clipfarm_get_timeline', description: 'Read tracks, clips, captions, scenes and silences' },
  { name: 'clipfarm_list_assets', description: 'List imported media with durations' },
  { name: 'clipfarm_place_clip', description: 'Place an asset at a timeline position' },
  { name: 'clipfarm_trim_clip', description: 'Trim a clip and keep source in/out aligned' },
  { name: 'clipfarm_split_clip', description: 'Split a clip at any absolute time' },
  { name: 'clipfarm_delete_clip', description: 'Remove a clip from the timeline' },
  { name: 'clipfarm_add_caption', description: 'Write a caption on the captions track' },
  { name: 'clipfarm_remove_silences', description: 'Detect and ripple-cut dead air' },
  { name: 'clipfarm_detect_scenes', description: 'Store PySceneDetect boundaries' },
  { name: 'clipfarm_transcribe', description: 'Generate word-timed captions' },
  { name: 'clipfarm_export', description: 'Render the timeline with FFmpeg' }
]

function Step({ index, title, children }: { index: number; title: string; children: ReactNode }): ReactNode {
  return (
    <div className="flex gap-3">
      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-elevated text-[10.5px] font-semibold text-secondary">
        {index}
      </span>
      <div className="min-w-0">
        <h3 className="text-[12.5px] font-semibold text-label">{title}</h3>
        <div className="mt-1 text-[11.5px] leading-relaxed text-secondary">{children}</div>
      </div>
    </div>
  )
}

export function AgentBridge(): ReactNode {
  const { state, actions } = useStudio()
  const [config, setConfig] = useState<McpConfig | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!state.project) {
      setConfig(null)
      return
    }
    let cancelled = false
    api
      .mcpConfig(state.project.id)
      .then((value) => {
        if (!cancelled) setConfig(value)
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : 'Could not build the MCP config')
      })
    return () => {
      cancelled = true
    }
  }, [state.project?.id])

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-[820px] px-8 pb-16 pt-8">
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-[24px] font-semibold tracking-[-0.02em] text-label">Agent Bridge</h1>
              <Badge tone="accent">MCP</Badge>
            </div>
            <p className="mt-1 max-w-[520px] text-[12.5px] leading-relaxed text-secondary">
              Let Claude Code, Codex or any MCP client read and edit this timeline directly. The server
              speaks JSON-RPC over stdio and writes to the same project file the app autosaves.
            </p>
          </div>
        </div>

        {!state.project ? (
          <div className="mt-8 rounded-[12px] border border-separator bg-surface">
            <EmptyState
              icon={<Bot size={20} aria-hidden />}
              title="Open a project first"
              message="The bridge exposes one project at a time so the agent can never edit the wrong timeline."
              action={
                <Button variant="primary" onClick={() => actions.setView('library')}>
                  Go to Library
                </Button>
              }
            />
          </div>
        ) : (
          <div className="mt-7 flex flex-col gap-5">
            <section className="rounded-[12px] border border-separator bg-surface p-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="flex h-7 w-7 items-center justify-center rounded-[7px] bg-accent-soft text-accent">
                    <Terminal size={14} aria-hidden />
                  </span>
                  <div>
                    <div className="text-[12.5px] font-semibold text-label">clipfarm MCP server</div>
                    <div className="text-[11px] text-secondary">
                      {isDemo ? 'Preview command — real paths appear in the desktop app' : 'stdio · local only'}
                    </div>
                  </div>
                </div>
                <Badge tone="success">
                  <Check size={10} aria-hidden /> Ready
                </Badge>
              </div>
              <pre className="mt-3 overflow-x-auto rounded-[8px] border border-separator bg-elevated px-3 py-2 font-mono text-[11px] leading-relaxed text-label">
                {config ? `${config.command} ${config.args.join(' ')}` : 'Building configuration…'}
              </pre>
            </section>

            <div className="grid gap-5 md:grid-cols-[1.15fr_1fr]">
              <section className="flex flex-col gap-4 rounded-[12px] border border-separator bg-surface p-4">
                <h2 className="text-[12.5px] font-semibold text-label">Connect an assistant</h2>
                <Step index={1} title="Copy the config">
                  Save the project once so its JSON lives in the clipfarm data folder.
                </Step>
                <Step index={2} title="Add it to your MCP client">
                  Paste the snippet into your client's server configuration and restart the client.
                </Step>
                <Step index={3} title="Ask in plain language">
                  “Remove the dead air from the interview, then add captions and export a 1080p cut.”
                </Step>
                <div className="mt-1 flex items-center gap-2 rounded-[8px] border border-separator bg-elevated px-2.5 py-2 text-[11px] text-secondary">
                  <ShieldCheck size={13} aria-hidden className="shrink-0 text-success" />
                  Local stdio only. No network listener, no account, no uploads.
                </div>
              </section>

              <section className="overflow-hidden rounded-[12px] border border-separator bg-surface">
                <div className="flex items-center justify-between border-b border-separator px-3.5 py-2.5">
                  <h2 className="text-[12.5px] font-semibold text-label">Client configuration</h2>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<Copy size={12} aria-hidden />}
                    disabled={!config}
                    onClick={() => config && void actions.copy(config.config, 'MCP config')}
                  >
                    Copy
                  </Button>
                </div>
                <pre className="max-h-[260px] overflow-auto px-3.5 py-3 font-mono text-[10.5px] leading-[1.6] text-secondary">
                  {config?.config ??
                    JSON.stringify({ mcpServers: { clipfarm: { command: 'python3', args: ['…'] } } }, null, 2)}
                </pre>
              </section>
            </div>

            {error ? (
              <div className="rounded-[8px] border border-[color-mix(in_srgb,var(--warning)_35%,transparent)] bg-[color-mix(in_srgb,var(--warning)_10%,transparent)] px-3 py-2 text-[11.5px] text-warning">
                {error}
              </div>
            ) : null}

            <section className="overflow-hidden rounded-[12px] border border-separator bg-surface">
              <header className="flex items-center justify-between border-b border-separator px-3.5 py-2.5">
                <h2 className="text-[12.5px] font-semibold text-label">Tools exposed to agents</h2>
                <span className="text-[11px] text-tertiary">{TOOLS.length} tools</span>
              </header>
              <ul className="divide-y divide-separator">
                {TOOLS.map((tool) => (
                  <li key={tool.name} className="flex items-baseline justify-between gap-4 px-3.5 py-2">
                    <code className="shrink-0 font-mono text-[11px] text-accent">{tool.name}</code>
                    <span className="truncate text-right text-[11px] text-secondary">{tool.description}</span>
                  </li>
                ))}
              </ul>
            </section>

            <p className="flex items-start gap-2 text-[11px] leading-relaxed text-tertiary">
              <ExternalLink size={12} aria-hidden className="mt-0.5 shrink-0" />
              Every bridge mutation is written to the project file, so the app picks it up on its next
              autosave cycle and the history stays reviewable.
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
