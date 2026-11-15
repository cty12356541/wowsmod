(async () => {
  // Run only from the current Vite source server. This probes the actual
  // buildMaterial/onBeforeCompile shader, not a second CPU implementation.
  const THREE = await import('/node_modules/three/build/three.module.js');
  const { Viewer } = await import('/src/engine/viewer.ts');
  const host = document.createElement('div');
  document.body.appendChild(host);
  const v = new Viewer(host, { quality: 'low', fixed: [16, 16], deterministic: true });
  const scene = new THREE.Scene();
  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 10);
  camera.position.z = 1;
  const geometry = new THREE.PlaneGeometry(2, 2);
  const mesh = new THREE.Mesh(geometry);
  scene.add(mesh);
  const target = new THREE.WebGLRenderTarget(1, 1, { type: THREE.UnsignedByteType, depthBuffer: false, stencilBuffer: false });
  const texture = (values) => {
    const t = new THREE.DataTexture(new Float32Array([...values, 1]), 1, 1, THREE.RGBAFormat, THREE.FloatType);
    t.encoding = THREE.LinearEncoding; t.minFilter = t.magFilter = THREE.NearestFilter;
    t.generateMipmaps = false; t.needsUpdate = true;
    return t;
  };
  const albedo = texture([0.05, 0.2, 0.1]);
  const rows = [];
  const shaderErrors = [];
  const originalError = console.error;
  console.error = (...args) => { shaderErrors.push(args.map(String).join(' ')); originalError(...args); };
  const sample = (name, fx, mg, expected, { output = 'vec3(roughnessFactor, metalnessFactor, wmMG.b)', gamma = 1, swap = 0, params = {} } = {}) => {
    const map = texture(mg);
    v.U.uMgGamma.value = gamma; v.U.uMgSwap.value = swap;
    const m = v.buildMaterial({ family: 'emissive', fx, params, textures: {} }, { a: albedo, mg: map }, name);
    const original = m.onBeforeCompile;
    m.onBeforeCompile = (shader, renderer) => {
      original(shader, renderer);
      // Replace only the output, after the real production material decode.
      const end = shader.fragmentShader.lastIndexOf('}');
      shader.fragmentShader = shader.fragmentShader.slice(0, end) + '\ngl_FragColor = vec4(' + output + ', 1.0);\n' + shader.fragmentShader.slice(end);
    };
    const originalKey = m.customProgramCacheKey;
    m.customProgramCacheKey = () => originalKey() + '|probe|' + output;
    mesh.material = m;
    v.renderer.setRenderTarget(target); v.renderer.render(scene, camera);
    const bytes = new Uint8Array(4);
    v.renderer.readRenderTargetPixels(target, 0, 0, 1, 1, bytes);
    const actual = Array.from(bytes.slice(0, 3), (value) => value / 255);
    const ok = actual.every((value, i) => Math.abs(value - expected[i]) <= 1.1 / 255) && bytes[3] === 255;
    rows.push({ name, fx, mg, gamma, swap, params, output, actual, expected, ok });
    m.dispose(); map.dispose();
  };
  try {
    for (const fx of ['ship_material.fx', 'ship_material_skinned.fx', 'ship_emissive_material_skinned.fx']) {
      sample('modern R gloss / G metal / B retained', fx, [0.25, 0.2, 0.4], [0.75, 0.2, 0.4]);
    }
    const modern = 'ship_emissive_material_skinned.fx';
    const legacy = 'PBS_ship_emissive.fx';
    sample('modern R changes only roughness', modern, [0.75, 0.2, 0.4], [0.25, 0.2, 0.4]);
    sample('modern G changes only metal', modern, [0.25, 0.3, 0.4], [0.75, 0.3, 0.4]);
    sample('explicit swap remains diagnostic', modern, [0.25, 0.2, 0.4], [0.8, 0.25, 0.4], { swap: 1 });
    for (const fx of [legacy, 'PBS_ship_emissive_skinned.fx']) {
      sample('legacy reflected defaults', fx, [0.25, 0.2, 0.4], [0.6464466094, 0.1296, 0.4]);
    }
    sample('legacy R remap independent of G', legacy, [0.75, 0.2, 0.4], [0.1940725511, 0.1296, 0.4]);
    sample('legacy G extraction independent of R', legacy, [0.25, 0.3, 0.4], [0.6464466094, 0.6561, 0.4]);
    sample('legacy gamma override before mul/pow', legacy, [0.25, 0.4, 0.4], [0.6464466094, 0.05308416, 0.4], { gamma: 2 });
    sample('legacy metal saturates after extraction', legacy, [0.25, 0.4, 0.4], [0.6464466094, 1, 0.4]);
    sample('legacy explicit zero parameters', legacy, [0.25, 0.2, 0.4], [0.005, 0, 0.4], { params: { g_legacyGlossRemap: 0, g_legacySpecularMul: 0 } });
    sample('legacy F0 uses normalized tint', legacy, [0.25, 0.2, 0.4], [0.0781813584, 0.164416, 0.1144846315], { output: 'material.specularColor' });
    sample('legacy diffuse applied once', legacy, [0.25, 0.2, 0.4], [0.06528, 0.26112, 0.13056], { output: 'material.diffuseColor' });
    sample('modern F0 remains standard mix', modern, [0.25, 0.2, 0.4], [0.042, 0.072, 0.052], { output: 'material.specularColor' });
    sample('modern diffuse remains standard mix', modern, [0.25, 0.2, 0.4], [0.04, 0.16, 0.08], { output: 'material.diffuseColor' });
    return { ok: rows.every((row) => row.ok) && shaderErrors.length === 0, rows, shaderErrors, note: 'Byte readback tolerance 1.1/255; linear synthetic albedo; no game-runtime gamma claim.' };
  } finally {
    console.error = originalError;
    v.renderer.setRenderTarget(null); geometry.dispose(); albedo.dispose(); target.dispose(); v.dispose(); host.remove();
  }
})()
