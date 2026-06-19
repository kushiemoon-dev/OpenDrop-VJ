{
  "targets": [
    {
      "target_name": "spout_addon",
      "conditions": [
        ["OS=='win'", {
          "sources": [
            "src/spout_addon.cc",
            "vendor/SpoutDX.cpp",
            "vendor/SpoutSenderNames.cpp",
            "vendor/SpoutSharedMemory.cpp",
            "vendor/SpoutFrameCount.cpp",
            "vendor/SpoutUtils.cpp",
            "vendor/SpoutCopy.cpp",
            "vendor/SpoutDirectX.cpp"
          ],
          "include_dirs": [
            "<!@(node -p \"require('node-addon-api').include_dir\")",
            "vendor"
          ],
          "defines": ["NAPI_DISABLE_CPP_EXCEPTIONS"],
          "libraries": ["d3d11.lib", "dxgi.lib"],
          "msvs_settings": {
            "VCCLCompilerTool": {
              "ExceptionHandling": 1
            }
          }
        }],
        ["OS!='win'", {
          "sources": ["src/spout_stub.cc"],
          "include_dirs": [
            "<!@(node -p \"require('node-addon-api').include_dir\")"
          ],
          "defines": ["NAPI_DISABLE_CPP_EXCEPTIONS"]
        }]
      ]
    }
  ]
}
