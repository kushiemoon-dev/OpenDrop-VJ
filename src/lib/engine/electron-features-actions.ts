/**
 * electron-features-actions.ts — connect/disconnect logic for the
 * Electron-only feature toggles (NDI/OSC/Remote/Ableton Link/v4l2/Spout).
 * Extracted from +page.svelte — pure orchestration calling
 * window.electronAPI (a browser-API boundary never unit tested in this
 * codebase, same precedent as the other toggle/connect functions still in
 * +page.svelte). Mutates electron-features-store.svelte.ts directly.
 *
 * toggleLink takes `clock` as a parameter rather than importing a shared
 * instance — Clock is still a +page.svelte-local singleton.
 */

import type { Clock } from './clock.js'
import { electronFeaturesState } from './electron-features-store.svelte.js'
import { audioSourceState } from './audio-source-store.svelte.js'

export async function toggleNdi(): Promise<void> {
  electronFeaturesState.ndi.error = ''
  const eAPI = window.electronAPI
  if (electronFeaturesState.ndi.active) {
    await eAPI?.ndiStop()
    electronFeaturesState.ndi.active = false
  } else {
    const w = window.screen.width
    const h = window.screen.height
    const res = await eAPI?.ndiStart('OpenDrop VJ', w, h)
    if (res?.ok) electronFeaturesState.ndi.active = true
    else
      electronFeaturesState.ndi.error =
        res?.error ?? 'NDI SDK not found; install the NDI Runtime from ndi.video.'
  }
}

export async function toggleV4l2(): Promise<void> {
  electronFeaturesState.v4l2.error = ''
  const eAPI = window.electronAPI
  if (electronFeaturesState.v4l2.active) {
    await eAPI?.v4l2Stop()
    electronFeaturesState.v4l2.active = false
  } else {
    const res = await eAPI?.v4l2Start()
    if (res?.ok) electronFeaturesState.v4l2.active = true
    else electronFeaturesState.v4l2.error = res?.error ?? 'Erreur v4l2 inconnue.'
  }
}

export async function toggleSpout(): Promise<void> {
  electronFeaturesState.spout.error = ''
  const eAPI = window.electronAPI
  if (electronFeaturesState.spout.active) {
    await eAPI?.spoutStop()
    electronFeaturesState.spout.active = false
  } else {
    const res = await eAPI?.spoutStart('OpenDrop VJ')
    if (res?.ok) electronFeaturesState.spout.active = true
    else electronFeaturesState.spout.error = res?.error ?? 'Spout indisponible.'
  }
}

export async function toggleOsc(): Promise<void> {
  electronFeaturesState.osc.error = ''
  const eAPI = window.electronAPI
  if (electronFeaturesState.osc.active) {
    await eAPI?.stopOsc?.()
    electronFeaturesState.osc.active = false
  } else {
    const res = await eAPI?.startOsc?.(electronFeaturesState.osc.port)
    if (res?.ok) electronFeaturesState.osc.active = true
    else electronFeaturesState.osc.error = res?.error ?? 'Erreur OSC.'
  }
}

export async function toggleRemote(): Promise<void> {
  electronFeaturesState.remote.error = ''
  const eAPI = window.electronAPI
  if (electronFeaturesState.remote.active) {
    await eAPI?.stopRemote?.()
    electronFeaturesState.remote.active = false
    electronFeaturesState.remote.url = ''
  } else {
    const res = await eAPI?.startRemote?.()
    if (res?.ok) {
      electronFeaturesState.remote.active = true
      electronFeaturesState.remote.url = `https://opendrop.kushie.dev/remote?host=${res.ip}&port=${res.port}&token=${res.token}`
    } else {
      electronFeaturesState.remote.error = res?.error ?? 'Erreur Remote.'
    }
  }
}

export async function toggleLink(clock: Clock): Promise<void> {
  electronFeaturesState.link.error = ''
  const eAPI = window.electronAPI
  if (electronFeaturesState.link.active) {
    await eAPI?.stopLink?.()
    electronFeaturesState.link.active = false
    electronFeaturesState.link.peers = 0
  } else {
    const res = await eAPI?.startLink?.(audioSourceState.manualBpm || clock.bpm || 120)
    if (res?.ok) {
      electronFeaturesState.link.active = true
      if (res.tempo) clock.setBpm(res.tempo)
    } else {
      electronFeaturesState.link.error = res?.error ?? 'Ableton Link non disponible.'
    }
  }
}
