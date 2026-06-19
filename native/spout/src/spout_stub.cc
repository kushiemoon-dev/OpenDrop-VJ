/**
 * spout_stub.cc — no-op module for non-Windows builds.
 * Exports the same API surface as spout_addon.cc so require() works on all platforms,
 * but all functions return false/undefined immediately.
 */

#include <napi.h>

static Napi::Value Init(const Napi::CallbackInfo& info) {
  return Napi::Boolean::New(info.Env(), false);
}

static Napi::Value Send(const Napi::CallbackInfo& info) {
  return Napi::Boolean::New(info.Env(), false);
}

static Napi::Value Stop(const Napi::CallbackInfo& info) {
  return info.Env().Undefined();
}

static Napi::Object InitModule(Napi::Env env, Napi::Object exports) {
  exports.Set("init", Napi::Function::New(env, Init));
  exports.Set("send", Napi::Function::New(env, Send));
  exports.Set("stop", Napi::Function::New(env, Stop));
  return exports;
}

NODE_API_MODULE(spout_addon, InitModule)
