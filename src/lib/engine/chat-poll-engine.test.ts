import { describe, it, expect } from 'vitest'
import { parseVote, createPoll, castVote, tally, resolveWinner } from './chat-poll-engine.js'

describe('parseVote', () => {
  it('parses a valid numeric token as a 0-based index', () => {
    expect(parseVote('1', 3)).toBe(0)
    expect(parseVote('3', 3)).toBe(2)
  })

  it('rejects out-of-range or non-numeric content', () => {
    expect(parseVote('4', 3)).toBeNull()
    expect(parseVote('0', 3)).toBeNull()
    expect(parseVote('lol nice preset', 3)).toBeNull()
  })

  it('ignores extra text around the vote token', () => {
    expect(parseVote('  2  ', 3)).toBe(1)
  })
})

describe('poll tally/dedupe/resolution', () => {
  it('counts one vote per option', () => {
    let poll = createPoll(3)
    poll = castVote(poll, 'user-1', 0)
    poll = castVote(poll, 'user-2', 1)
    expect(tally(poll)).toEqual([1, 1, 0])
  })

  it('a second vote from the same user replaces their first, not adds to it', () => {
    let poll = createPoll(3)
    poll = castVote(poll, 'user-1', 0)
    poll = castVote(poll, 'user-1', 2)
    expect(tally(poll)).toEqual([0, 0, 1])
  })

  it('resolves the option with the most votes', () => {
    let poll = createPoll(2)
    poll = castVote(poll, 'user-1', 1)
    poll = castVote(poll, 'user-2', 1)
    poll = castVote(poll, 'user-3', 0)
    expect(resolveWinner(poll)).toBe(1)
  })

  it('breaks ties by the lowest option index', () => {
    let poll = createPoll(2)
    poll = castVote(poll, 'user-1', 1)
    poll = castVote(poll, 'user-2', 0)
    expect(resolveWinner(poll)).toBe(0)
  })

  it('returns null when nobody voted', () => {
    const poll = createPoll(2)
    expect(resolveWinner(poll)).toBeNull()
  })
})
