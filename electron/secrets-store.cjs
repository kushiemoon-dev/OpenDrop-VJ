'use strict'

const fs = require('fs')
const path = require('path')
const { app, safeStorage } = require('electron')

function secretsFilePath() {
  return path.join(app.getPath('userData'), 'secrets.enc')
}

function readAll() {
  const file = secretsFilePath()
  if (!fs.existsSync(file)) return {}
  try {
    const encrypted = fs.readFileSync(file)
    const json = safeStorage.decryptString(encrypted)
    return JSON.parse(json)
  } catch {
    return {}
  }
}

function writeAll(obj) {
  const json = JSON.stringify(obj)
  const encrypted = safeStorage.encryptString(json)
  fs.writeFileSync(secretsFilePath(), encrypted)
}

function getSecret(key) {
  if (!safeStorage.isEncryptionAvailable()) return null
  const all = readAll()
  return Object.prototype.hasOwnProperty.call(all, key) ? all[key] : null
}

function setSecret(key, value) {
  if (!safeStorage.isEncryptionAvailable()) throw new Error('OS keychain encryption unavailable.')
  const all = readAll()
  all[key] = value
  writeAll(all)
}

function hasSecret(key) {
  const all = readAll()
  return Object.prototype.hasOwnProperty.call(all, key) && !!all[key]
}

function clearSecret(key) {
  const all = readAll()
  delete all[key]
  writeAll(all)
}

module.exports = { getSecret, setSecret, hasSecret, clearSecret }
