/**
 * midi-connection-actions.ts — connect/disconnect logic for the MIDI
 * hardware connection, including message dispatch (soft-takeover, LED
 * confirmation flash) and MIDI-clock-IN → Clock BPM detection. Extracted
 * from +page.svelte — pure orchestration calling the MidiEngine browser-API
 * boundary, never unit tested in this codebase (same precedent as
 * electron-features-actions.ts / audio-source-actions.ts). Mutates
 * midi-connection-store.svelte.ts and midi-mapping-store.svelte.ts directly.
 *
 * `midi` is NOT held module-locally — unlike audio-source-actions.ts's
 * loopbackUnlisten, +page.svelte itself still reads `midi` (onDestroy,
 * pushLedStates), so this function takes the current instance and returns
 * the new one; the caller is responsible for reassigning its own `let midi`.
 */

import { MidiEngine, triggerKey, type MidiTriggerKey } from './midi.js'
import type { CommandId, CommandRegistry, CommandContext } from './commands.js'
import type { Clock } from './clock.js'
import { midiConnectionState } from './midi-connection-store.svelte.js'
import { midiMappingState, setMidiMapping } from './midi-mapping-store.svelte.js'
import { runStatusState } from './run-status-store.svelte.js'

export interface ToggleMidiDeps {
  registry: CommandRegistry
  commandCtx: CommandContext
  clock: Clock
  getCommandCurrentValue: (id: CommandId) => number | null
  getCommandLedState: (id: CommandId) => boolean | null
  pushLedStates: () => void
}

export async function toggleMidi(
  midi: MidiEngine | null,
  deps: ToggleMidiDeps
): Promise<MidiEngine | null> {
  const { registry, commandCtx, clock, getCommandCurrentValue, getCommandLedState, pushLedStates } =
    deps

  if (midiConnectionState.connected) {
    midi?.destroy()
    midiConnectionState.connected = false
    midiConnectionState.deviceNames = []
    midiMappingState.learningAction = null
    midiConnectionState.clockBpm = 0
    return null
  }

  try {
    const newMidi = new MidiEngine()
    await newMidi.connect()
    midiConnectionState.connected = true
    midiConnectionState.deviceNames = newMidi.deviceNames
    newMidi.onOutputReconnect(() => pushLedStates())
    pushLedStates() // initial LED state at connection time

    // Soft-takeover: Set<key> of controls already in phase with the app value
    const takenOver = new Set<MidiTriggerKey>()

    newMidi.onMessage((msg) => {
      const key = triggerKey(msg)

      if (midiMappingState.learningAction !== null) {
        if (msg.type === 'note_off') return
        setMidiMapping(midiMappingState.learningAction, key)
        takenOver.add(key) // immediately in phase after learn
        midiMappingState.learningAction = null
        return
      }

      for (const [action, mapped] of Object.entries(midiMappingState.midiMappings) as [
        CommandId,
        MidiTriggerKey,
      ][]) {
        if (mapped !== key) continue
        if (msg.type === 'note_off') break

        // Normalize: 14-bit over 0..16383, otherwise 7-bit over 0..127
        const value01 = msg.is14bit ? msg.value / 16383 : msg.value / 127

        // Soft-takeover only applies to range commands
        const cmd = registry.get(action)
        if (cmd?.kind === 'range' && !takenOver.has(key)) {
          const current = getCommandCurrentValue(action)
          if (current !== null && Math.abs(value01 - current) > 0.08) break
          takenOver.add(key)
        }

        if (runStatusState.status === 'running') {
          registry.dispatch(action, value01, commandCtx)
          // Confirmation flash — excluded for commands with persistent state
          // (strobe-toggle, playlist-toggle-*): without this guard, the setTimeout
          // below would wrongly overwrite the state that pushLedStates() just
          // updated on the same tick (see Global Constraints).
          if (cmd?.kind === 'trigger' && getCommandLedState(action) === null) {
            newMidi.sendFeedback(key, true)
            setTimeout(() => newMidi.sendFeedback(key, false), 120)
          }
        }
        break
      }
    })

    // MIDI clock IN → feeds the Clock (24 pulses per quarter note)
    let clockPulses = 0
    let clockTsRing: number[] = []
    let clockTimer: ReturnType<typeof setTimeout> | null = null

    newMidi.onClock(() => {
      const now = performance.now()
      clockPulses++
      clockTsRing.push(now)
      if (clockTsRing.length > 49) clockTsRing.shift()

      // BPM update every 6 pulses (≈4× per beat at 120 BPM)
      if (clockPulses % 6 === 0 && clockTsRing.length >= 7) {
        const recent = clockTsRing.slice(-7)
        const intervals = recent.slice(1).map((t, i) => t - recent[i]!)
        const avg = intervals.reduce((a, b) => a + b, 0) / intervals.length
        const bpm = Math.round((60000 / (avg * 24)) * 10) / 10
        if (bpm >= 40 && bpm <= 300) {
          midiConnectionState.clockBpm = bpm
          clock.setBpm(bpm)
        }
      }

      // Beat on every quarter note (24 pulses)
      if (clockPulses % 24 === 0) clock.pulse()

      // Inactivity timeout: MIDI clock stopped for 2s
      if (clockTimer !== null) clearTimeout(clockTimer)
      clockTimer = setTimeout(() => {
        midiConnectionState.clockBpm = 0
        clockPulses = 0
        clockTsRing = []
        clockTimer = null
      }, 2000)
    })

    return newMidi
  } catch (e) {
    midiConnectionState.connected = false
    runStatusState.sourceError = e instanceof Error ? e.message : String(e)
    return null
  }
}
