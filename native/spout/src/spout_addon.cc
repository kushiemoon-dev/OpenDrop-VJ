/**
 * spout_addon.cc — N-API wrapper around SpoutDX (Windows only).
 *
 * Exposes three functions to Node.js:
 *   init(name: string) → bool   — open DX11 device + name the sender
 *   send(bgra: Buffer, w: number, h: number) → bool   — push one frame
 *   stop() → void   — release sender + DX11 device
 *
 * Pixel format expected: BGRA, top-down, stride = w * 4.
 * This matches Electron's webContents.capturePage().toBitmap() exactly.
 *
 * NOTE: Vendor sources (SpoutDX.cpp et al.) must be present in ../vendor/
 * before building. See vendor/SOURCE.txt for the upstream pinned commit.
 */

#ifdef _WIN32

#include <napi.h>
#include "SpoutDX.h"

static spoutDX g_spout;
static bool    g_initialized = false;

static Napi::Value Init(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  std::string name = info[0].As<Napi::String>().Utf8Value();

  // OpenDirectX11 creates an internal ID3D11Device.
  // Returns false on headless / no GPU — handled gracefully in JS.
  g_initialized = g_spout.OpenDirectX11();
  if (g_initialized) {
    g_spout.SetSenderName(name.c_str());
  }
  return Napi::Boolean::New(env, g_initialized);
}

static Napi::Value Send(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  if (!g_initialized) return Napi::Boolean::New(env, false);

  auto  buf = info[0].As<Napi::Buffer<uint8_t>>();
  auto  w   = static_cast<uint32_t>(info[1].As<Napi::Number>().Uint32Value());
  auto  h   = static_cast<uint32_t>(info[2].As<Napi::Number>().Uint32Value());

  // bInvert=false: capturePage already produces top-down BGRA
  bool ok = g_spout.SendImage(buf.Data(), w, h, DXGI_FORMAT_B8G8R8A8_UNORM, false);
  return Napi::Boolean::New(env, ok);
}

static Napi::Value Stop(const Napi::CallbackInfo& info) {
  g_spout.ReleaseSender();
  g_spout.CloseDirectX11();
  g_initialized = false;
  return info.Env().Undefined();
}

static Napi::Object InitModule(Napi::Env env, Napi::Object exports) {
  exports.Set("init", Napi::Function::New(env, Init));
  exports.Set("send", Napi::Function::New(env, Send));
  exports.Set("stop", Napi::Function::New(env, Stop));
  return exports;
}

NODE_API_MODULE(spout_addon, InitModule)

#endif // _WIN32
