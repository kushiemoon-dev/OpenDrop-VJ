/**
 * chat-poll-engine.ts — pure vote parsing/tallying/resolution. No I/O, no
 * platform knowledge (Twitch vs Kick is just a voterId namespace prefix
 * applied by the caller, see chat-poll-actions.ts) — fully unit-testable
 * without a live chat connection.
 */

export function parseVote(content: string, optionCount: number): number | null {
  const trimmed = content.trim()
  if (!/^\d+$/.test(trimmed)) return null
  const n = parseInt(trimmed, 10)
  if (n < 1 || n > optionCount) return null
  return n - 1
}

export interface PollState {
  votes: Map<string, number>
  optionCount: number
}

export function createPoll(optionCount: number): PollState {
  return { votes: new Map(), optionCount }
}

/** Last vote per voterId wins — returns a new PollState, does not mutate the input. */
export function castVote(poll: PollState, voterId: string, optionIndex: number): PollState {
  const votes = new Map(poll.votes)
  votes.set(voterId, optionIndex)
  return { ...poll, votes }
}

export function tally(poll: PollState): number[] {
  const counts = new Array(poll.optionCount).fill(0)
  for (const optionIndex of poll.votes.values()) counts[optionIndex]++
  return counts
}

/** Highest vote count wins; ties broken by lowest option index; null if zero votes cast. */
export function resolveWinner(poll: PollState): number | null {
  const counts = tally(poll)
  const total = counts.reduce((a, b) => a + b, 0)
  if (total === 0) return null
  let best = 0
  for (let i = 1; i < counts.length; i++) {
    if (counts[i]! > counts[best]!) best = i
  }
  return best
}
