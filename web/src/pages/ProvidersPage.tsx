import { useEffect, useRef, useState } from 'react'
import { Check, CircleNotch } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { type Health } from '@/lib/api'
import { cn } from '@/lib/utils'
import { Page, StatePanel, ErrorBanner } from '@/components/PageState'

interface DetectedModel {
  id: string
  name: string | null
  context_window: number | null
  max_output_tokens: number | null
}

interface ProviderPreset {
  id: string
  name: string
  base_url: string
  models_url: string
  key_url: string
}

export function ProvidersPage({ health, onSaved }: { health: Health; onSaved: () => Promise<void> }) {
  const [presets, setPresets] = useState<ProviderPreset[]>([])
  const [presetId, setPresetId] = useState('')
  const [provider, setProvider] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [modelsUrl, setModelsUrl] = useState('')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [hasApiKey, setHasApiKey] = useState(false)
  const [windowSize, setWindowSize] = useState(0)
  const [models, setModels] = useState<DetectedModel[]>([])
  const [mode, setMode] = useState<'manual' | 'auto'>('auto')
  const [loadingModels, setLoadingModels] = useState(false)
  const [modelRefresh, setModelRefresh] = useState(0)
  const [modelError, setModelError] = useState('')
  const [saved, setSaved] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const savedProvider = useRef({ preset: '', name: '', baseUrl: '', modelsUrl: '', model: '', contextWindow: 0, hasApiKey: false })
  const discovery = useRef<AbortController | null>(null)
  const preset = presets.find((entry) => entry.id === presetId)
  const custom = presetId === 'custom'
  const providerActive = Boolean(presetId && provider.trim() && baseUrl.trim())
  const connected = providerActive && presetId === savedProvider.current.preset
    && provider.trim() === savedProvider.current.name
    && baseUrl.trim().replace(/\/+$/, '') === savedProvider.current.baseUrl

  useEffect(() => {
    const controller = new AbortController()
    void Promise.all([
      fetch('/api/providers', { signal: controller.signal }).then(async (r) => {
        if (!r.ok) throw new Error(await r.text())
        return r.json() as Promise<{ providers: ProviderPreset[] }>
      }),
      fetch('/api/config', { signal: controller.signal }).then(async (r) => {
        if (!r.ok) throw new Error(await r.text())
        return r.json() as Promise<{ provider: string; preset: string; base_url: string; models_url: string; model: string; has_api_key: boolean; context_window: number }>
      }),
    ])
      .then(([catalogue, config]) => {
        if (controller.signal.aborted) return
        const activePreset = config.preset || (config.provider && config.base_url ? 'custom' : '')
        setPresets(catalogue.providers)
        setPresetId(activePreset)
        setProvider(config.provider)
        setBaseUrl(config.base_url)
        setModelsUrl(config.models_url)
        setModel(config.model)
        setWindowSize(config.context_window)
        setHasApiKey(config.has_api_key)
        savedProvider.current = { preset: activePreset, name: config.provider, baseUrl: config.base_url.replace(/\/+$/, ''), modelsUrl: config.models_url, model: config.model, contextWindow: config.context_window, hasApiKey: config.has_api_key }
      })
      .catch((cause) => { if (!controller.signal.aborted) setError(cause instanceof Error ? cause.message : String(cause)) })
      .finally(() => { if (!controller.signal.aborted) setLoading(false) })
    return () => controller.abort()
  }, [])

  useEffect(() => {
    const controller = new AbortController()
    discovery.current = controller
    setModels([])
    setModelError('')
    setLoadingModels(false)
    if (loading || !connected || mode !== 'auto' || !modelsUrl.trim()) return
    setLoadingModels(true)
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const response = await fetch('/api/models', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ provider, base_url: baseUrl, models_url: modelsUrl, api_key: apiKey || undefined, use_saved_key: !apiKey && hasApiKey }),
            signal: controller.signal,
          })
          const body = await response.json() as { models: DetectedModel[]; error?: string }
          if (!response.ok) throw new Error(body.error || `Model detection failed (${response.status})`)
          if (!controller.signal.aborted) setModels(body.models)
        } catch (cause) {
          if (!controller.signal.aborted) setModelError(cause instanceof Error ? cause.message : String(cause))
        } finally {
          if (!controller.signal.aborted) setLoadingModels(false)
        }
      })()
    }, 300)
    return () => { clearTimeout(timer); controller.abort() }
  }, [loading, connected, mode, provider, baseUrl, modelsUrl, apiKey, hasApiKey, modelRefresh])

  function choosePreset(entry: ProviderPreset) {
    discovery.current?.abort()
    setApiKey('')
    setPresetId(entry.id)
    setMode('auto')
    setModels([])
    setError('')
    if (entry.id === savedProvider.current.preset) {
      setProvider(savedProvider.current.name)
      setBaseUrl(savedProvider.current.baseUrl)
      setModelsUrl(savedProvider.current.modelsUrl)
      setModel(savedProvider.current.model)
      setWindowSize(savedProvider.current.contextWindow)
      setHasApiKey(savedProvider.current.hasApiKey)
      return
    }
    setProvider(entry.id === 'custom' ? '' : entry.name)
    setBaseUrl(entry.base_url)
    setModelsUrl(entry.models_url)
    setModel('')
    setWindowSize(0)
    setApiKey('')
    setHasApiKey(false)
  }

  function changeCustomProvider(setValue: (value: string) => void, value: string) {
    discovery.current?.abort()
    setModels([])
    setModelsUrl('')
    setModel('')
    setWindowSize(0)
    setApiKey('')
    setHasApiKey(false)
    setValue(value)
  }

  async function connectProvider() {
    setError('')
    setSaving(true)
    try {
      const response = await fetch('/api/provider', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          preset: presetId,
          provider,
          base_url: baseUrl,
          models_url: modelsUrl,
          api_key: apiKey || undefined,
        }),
      })
      if (!response.ok) {
        const body = (await response.json()) as { error?: string }
        throw new Error(body.error || 'Could not connect the provider')
      }
      const keySaved = Boolean(apiKey.trim()) || (connected && hasApiKey)
      setHasApiKey(keySaved)
      savedProvider.current = { preset: presetId, name: provider.trim(), baseUrl: baseUrl.trim().replace(/\/+$/, ''), modelsUrl, model: '', contextWindow: 0, hasApiKey: keySaved }
      setApiKey('')
      setModel('')
      setWindowSize(0)
      setModels([])
      setMode('auto')
      await onSaved()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setSaving(false)
    }
  }


  async function activateModel(entry: DetectedModel) {
    if (saving || !connected) return
    setSaved(false)
    setError('')
    setSaving(true)
    try {
      const response = await fetch('/api/model', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, base_url: baseUrl, model: entry.id, models_url: modelsUrl, context_window: entry.context_window ?? undefined }),
      })
      const result = (await response.json()) as { context_window?: number; error?: string }
      if (!response.ok) throw new Error(result.error || 'Could not activate the model')
      setModel(entry.id)
      if (result.context_window) setWindowSize(result.context_window)
      savedProvider.current.model = entry.id
      savedProvider.current.modelsUrl = modelsUrl
      if (result.context_window) savedProvider.current.contextWindow = result.context_window
      setSaved(true)
      await onSaved()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setSaving(false)
    }
  }

  if (loading) return <Page><StatePanel loading title="Loading providers" body="Reading the supported provider list." /></Page>

  return (
    <Page title="Providers" description="Pick a provider, then add its key. Models belong to the active provider only.">
      <fieldset disabled={saving} className="min-w-0">
      <form
        className="max-w-2xl space-y-5 rounded-[var(--radius-lg)] border border-border bg-card p-5"
        onChange={() => { setSaved(false); setError('') }}
        onSubmit={async (event) => {
          event.preventDefault()
          setSaved(false)
          setSaving(true)
          setError('')
          try {
            const response = await fetch('/api/config', {
              method: 'PUT',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ provider, preset: presetId, base_url: baseUrl, models_url: modelsUrl, model, api_key: apiKey || undefined, context_window: windowSize }),
            })
            if (!response.ok) {
              const body = (await response.json()) as { error?: string }
              throw new Error(body.error || 'Could not save the model')
            }
            if (apiKey) setHasApiKey(true)
            savedProvider.current.model = model.trim()
            savedProvider.current.contextWindow = windowSize
            savedProvider.current.modelsUrl = modelsUrl
            savedProvider.current.hasApiKey = Boolean(apiKey.trim()) || hasApiKey
            setApiKey('')
            await onSaved()
            setSaved(true)
          } catch (cause) {
            setError(cause instanceof Error ? cause.message : String(cause))
          } finally {
            setSaving(false)
          }
        }}
      >
        <fieldset className="space-y-3">
          <legend className="text-sm font-medium">Provider</legend>
          <div className="grid gap-2 sm:grid-cols-2">
            {presets.map((entry) => (
              <button
                key={entry.id}
                type="button"
                aria-pressed={entry.id === presetId}
                onClick={() => choosePreset(entry)}
                className={cn(
                  'min-h-11 rounded-[var(--radius-sm)] border px-3 py-2 text-left text-sm transition-colors',
                  entry.id === presetId ? 'border-primary bg-primary/10 text-foreground' : 'border-border hover:bg-muted',
                )}
              >
                <span className="block font-medium">{entry.name}</span>
                <span className="block text-xs text-muted-foreground">{entry.id === 'custom' ? 'Enter your own endpoint' : new URL(entry.base_url).host}</span>
              </button>
            ))}
          </div>
        </fieldset>
        {!presetId ? (
          <p className="text-sm text-muted-foreground">Choose a provider to continue.</p>
        ) : (
          <>
            {custom ? (
              <>
                <Field label="Provider name" value={provider} setValue={(value) => changeCustomProvider(setProvider, value)} placeholder="Provider name" />
                <Field label="Base URL" value={baseUrl} setValue={(value) => changeCustomProvider(setBaseUrl, value)} placeholder="https://provider.example/v1" />
              </>
            ) : (
              <p className="text-xs text-muted-foreground">Endpoint: {baseUrl}{preset?.key_url ? <> · <a className="underline" href={preset.key_url} target="_blank" rel="noreferrer">Get an API key</a></> : null}</p>
            )}
            <Field label="API key" value={apiKey} setValue={setApiKey} placeholder={hasApiKey && presetId === savedProvider.current.preset ? 'Leave blank to keep the saved key' : 'Paste the provider API key'} type="password" />
            {!connected ? (
              <>
                <Button type="button" disabled={saving || !providerActive || (!custom && !apiKey.trim())} onClick={() => void connectProvider()}>
                  {saving ? 'Connecting…' : 'Connect provider'}
                </Button>
                <p className="text-sm text-muted-foreground">Connect this provider to add its models.</p>
              </>
            ) : (
              <fieldset className="space-y-4 border-t border-border pt-4">
                <legend className="px-1 text-sm font-medium">Model</legend>
                <div className="flex flex-wrap gap-3">
                  <Button type="button" variant={mode === 'auto' ? 'default' : 'outline'} aria-pressed={mode === 'auto'} onClick={() => setMode('auto')}>Auto detect</Button>
                  <Button type="button" variant={mode === 'manual' ? 'default' : 'outline'} aria-pressed={mode === 'manual'} onClick={() => setMode('manual')}>Manual</Button>
                </div>
                {mode === 'auto' ? (
                  <div className="space-y-3">
                    {custom ? <Field label="Model-list URL" value={modelsUrl} setValue={setModelsUrl} placeholder="https://provider.example/v1/models" /> : <p className="text-xs text-muted-foreground">Model list: {modelsUrl}</p>}
                    <Button type="button" variant="outline" disabled={loadingModels || !modelsUrl.trim()} onClick={() => setModelRefresh((value) => value + 1)}>
                      {loadingModels ? <CircleNotch className="size-4 animate-spin" aria-hidden /> : null}
                      {loadingModels ? 'Loading…' : 'Refresh models'}
                    </Button>
                    {modelError ? <p role="alert" className="break-words text-sm text-destructive">{modelError}. Check the key or use Manual.</p> : null}
                    {models.length ? (
                      <ul className="divide-y divide-border overflow-hidden rounded-[var(--radius-sm)] border border-border">
                        {models.map((entry) => {
                          const active = entry.id === savedProvider.current.model
                          return (
                            <li key={entry.id}>
                              <button
                                type="button"
                                aria-current={active}
                                disabled={saving}
                                onClick={() => void activateModel(entry)}
                                className={cn(
                                  'flex min-h-11 w-full flex-wrap items-baseline justify-between gap-x-4 gap-y-1 px-3 py-2 text-left text-sm transition-colors',
                                  active ? 'bg-primary/10 text-foreground' : 'hover:bg-muted',
                                )}
                              >
                                <span className="break-all font-medium">{entry.id}</span>
                                <span className="text-xs text-muted-foreground">
                                  {entry.context_window ? `${entry.context_window.toLocaleString()} tokens` : 'context not reported'}
                                  {entry.max_output_tokens ? ` · ${entry.max_output_tokens.toLocaleString()} output` : ''}
                                  {active ? ' · in use' : ''}
                                </span>
                              </button>
                            </li>
                          )
                        })}
                      </ul>
                    ) : null}
                    <p className="text-xs text-muted-foreground" role="status">
                      {loadingModels ? 'Loading models…' : models.length ? `${models.length} models. Selecting one activates it immediately.` : 'No models loaded.'}
                    </p>
                    <p className="text-xs text-muted-foreground">If context metadata is not reported, the current configured window is retained.</p>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <Field label="Model ID" value={model} setValue={setModel} placeholder="Exact ID accepted by the provider" />
                    <Field label="Context window" value={String(windowSize)} setValue={(value) => setWindowSize(Number(value) || 0)} placeholder="Context size in tokens" type="number" />
                    <Button type="submit" disabled={saving || !model.trim() || windowSize <= 0}>{saving ? 'Saving…' : 'Save model'}</Button>
                  </div>
                )}
              </fieldset>
            )}
          </>
        )}
        {error ? <ErrorBanner message={error} /> : null}
        {saved ? <p role="status" className="text-sm text-[var(--success)]">Model active: {model}</p> : null}
      </form>
      </fieldset>
    </Page>
  )
}
function Field({ label, value, setValue, placeholder, type = 'text' }: { label: string; value: string; setValue: (value: string) => void; placeholder: string; type?: string }) {
  return (
    <label className="block space-y-1.5">
      <span className="text-xs font-medium">{label}</span>
      <input
        type={type}
        value={value}
        onChange={(event) => setValue(event.target.value)}
        placeholder={placeholder}
        className="min-h-11 w-full rounded-[var(--radius-sm)] border border-input bg-background px-3 text-sm outline-none transition-colors focus:border-ring"
      />
    </label>
  )
}
