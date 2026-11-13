// 视口用到的 GLSL 片段。船体材质 = MeshStandardMaterial + 注入（规则见 API.md §9）。

export const skyVS = `varying vec3 vDir; void main(){ vDir = normalize((modelMatrix*vec4(position,1.0)).xyz); gl_Position = projectionMatrix*modelViewMatrix*vec4(position,1.0); gl_Position.z = gl_Position.w; }`;
export const skyFS = `
uniform vec3 uZenith, uHorizon, uBelow, uSunDir, uSunCol; uniform float uSunDisk; varying vec3 vDir;
void main(){
  vec3 d = normalize(vDir); float y = d.y;
  vec3 col = y >= 0.0 ? mix(uHorizon, uZenith, smoothstep(0.0, 0.85, y)) : mix(uHorizon, uBelow, smoothstep(0.0, 0.18, -y));
  float s = max(dot(d, uSunDir), 0.0);
  col += uSunCol * (pow(s, 900.0) * 6.0 + pow(s, 18.0) * 0.10) * uSunDisk;
  gl_FragColor = vec4(col, 1.0);
}`;

// 世界坐标：船固定在原点，水面/地面按船在世界里的位置 uShipP 与航向 uShipPsi 反向流动。
const WORLD = `
uniform vec2 uShipP; uniform float uShipPsi;
vec2 toWorld(vec2 l){ float c = cos(uShipPsi), s = sin(uShipPsi); return vec2(c*l.x + s*l.y, -s*l.x + c*l.y) + uShipP; }`;

export const flatVS = `varying vec3 vW; void main(){ vec4 w = modelMatrix*vec4(position,1.0); vW = w.xyz; gl_Position = projectionMatrix*viewMatrix*w; }`;

export const waterFS = `
uniform float uTime, uCut, uFoamK, uWaterOpa; uniform vec2 uCutDir; uniform vec3 uSunDir, uSunCol, uZenith, uHorizon, uDeep, uShallow, uEdge;
${WORLD}
varying vec3 vW;
float wh(vec2 p){ return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float wn(vec2 p){ vec2 i = floor(p), f = fract(p); f = f*f*(3.0-2.0*f);
  return mix(mix(wh(i), wh(i+vec2(1,0)), f.x), mix(wh(i+vec2(0,1)), wh(i+vec2(1,1)), f.x), f.y); }
float wH(vec2 p, float t){ float h = 0.0, a = 0.5; mat2 R = mat2(0.8, 0.6, -0.6, 0.8);
  for (int i = 0; i < 4; i++) { h += a * wn(p + vec2(t*0.25, t*0.12)); p = R * p * 2.1; a *= 0.5; } return h; }
void main(){
  float side = dot(vW.xz, uCutDir);
  if (uCut > 0.5 && side > 0.0) discard;
  vec3 toC = cameraPosition - vW; float dist = length(toC); vec3 V = toC/dist;
  vec2 wp = toWorld(vW.xz);
  float e = 0.05, s = 0.045 * exp(-dist/30.0);
  vec2 q = wp * 1.6; float h0 = wH(q, uTime), hx = wH(q + vec2(e,0.0), uTime), hz = wH(q + vec2(0.0,e), uTime);
  vec3 N = normalize(vec3(-(hx-h0)/e*s, 1.0, -(hz-h0)/e*s));
  float fr = 0.02 + 0.98*pow(1.0 - abs(dot(N,V)), 5.0);
  vec3 R = reflect(-V, N);
  vec3 sky = mix(uHorizon, uZenith, smoothstep(0.0, 0.85, abs(R.y)));
  vec3 body = mix(uShallow, uDeep, 0.35);
  vec3 col = mix(body, sky, clamp(fr*1.2, 0.0, 1.0));
  col += uSunCol * pow(max(dot(R, uSunDir), 0.0), 400.0) * 1.6;
  float a = mix(0.10, 0.62, fr);
  a = clamp(a * uWaterOpa, 0.0, 0.92);  // waterOpa>1 → 更不透明(港口浑浊)
  a = mix(a, 1.0, smoothstep(25.0, 140.0, dist));
  col = mix(col, uHorizon, smoothstep(60.0, 260.0, dist));
  if (uCut > 0.5) { float ln = 1.0 - smoothstep(0.0, 0.02 + dist*0.0012, abs(side)); col = mix(col, uEdge, ln*0.9); a = max(a, ln*0.9); }
  gl_FragColor = vec4(col, a);
}`;

export const cutFS = `
uniform vec3 uDeep, uEdge; uniform float uWL, uR; varying vec3 vW;
void main(){
  float d = uWL - vW.y;
  float a = 0.16 * (1.0 - smoothstep(0.6, 2.6, d));
  float ln = 1.0 - smoothstep(0.0, 0.012, abs(d));
  float fx = 1.0 - smoothstep(uR, uR + 4.0, length(vW.xz));
  gl_FragColor = vec4(mix(uDeep, uEdge, ln), max(a, ln*0.85) * fx);
}`;

export const sheetFS = `
uniform vec3 uCol; uniform float uA; uniform vec2 uHalf; varying vec3 vW;
void main(){
  float d = length(vec2(vW.x/uHalf.x, vW.z/uHalf.y));
  float a = uA * (1.0 - smoothstep(0.55, 1.0, d));
  vec2 g = abs(fract(vW.xz) - 0.5); float ln = 1.0 - smoothstep(0.0, 0.015, 0.5 - max(g.x, g.y));
  gl_FragColor = vec4(uCol, a * (1.0 + ln*1.2));
}`;

// 影棚地面：细网格 + 航行时按船的世界位置流动（表达“在走”，不是水面）
export const floorFS = `
uniform vec3 uCol, uLine; uniform float uGrid, uFade, uLineA; varying vec3 vW;
${WORLD}
float gridLine(vec2 p, float w){ vec2 g = abs(fract(p - 0.5) - 0.5) / fwidth(p); return 1.0 - min(min(g.x, g.y) / w, 1.0); }
void main(){
  vec2 wp = toWorld(vW.xz);
  float r = length(vW.xz * vec2(1.0, 0.6));
  float fade = 1.0 - smoothstep(uFade * 0.35, uFade, r);
  float l1 = gridLine(wp / uGrid, 1.0), l2 = gridLine(wp / (uGrid * 5.0), 1.2);
  vec3 col = mix(uCol, uLine, clamp(l1 * 0.35 + l2 * 0.7, 0.0, 1.0) * uLineA);
  gl_FragColor = vec4(col, fade);
}`;

export const blobFS = `
uniform float uA; uniform vec2 uHalf; varying vec3 vW;
void main(){
  float d = length(vec2(vW.x/uHalf.x, vW.z/uHalf.y));
  gl_FragColor = vec4(0.06, 0.09, 0.12, uA * (1.0 - smoothstep(0.35, 1.05, d)));
}`;

// 尾迹 / 船首浪 / 舷侧白沫（近似示意）。aUv.x = 沿带方向 0..1，aUv.y = 横向 -1..1，aA = 强度
export const foamVS = `
attribute vec2 aUv; attribute float aA; varying vec2 vUv; varying float vA; varying vec3 vW;
void main(){ vUv = aUv; vA = aA; vec4 w = modelMatrix*vec4(position,1.0); vW = w.xyz; gl_Position = projectionMatrix*viewMatrix*w; }`;
export const foamFS = `
uniform float uTime, uK; uniform vec3 uFoam; varying vec2 vUv; varying float vA; varying vec3 vW;
${WORLD}
float fh(vec2 p){ return fract(sin(dot(p, vec2(41.3, 289.1))) * 15731.743); }
float fn(vec2 p){ vec2 i = floor(p), f = fract(p); f = f*f*(3.0-2.0*f);
  return mix(mix(fh(i), fh(i+vec2(1,0)), f.x), mix(fh(i+vec2(0,1)), fh(i+vec2(1,1)), f.x), f.y); }
void main(){
  vec2 wp = toWorld(vW.xz) * 9.0;
  float n = fn(wp + uTime*0.6) * 0.55 + fn(wp*2.3 - uTime*0.9) * 0.3 + fn(wp*5.1) * 0.15;
  float edge = 1.0 - smoothstep(0.55, 1.0, abs(vUv.y));
  float core = smoothstep(0.25, 0.0, abs(vUv.y)) * 0.35;
  float m = smoothstep(0.42 - vA * 0.25, 0.72, n + core) * edge;
  float a = m * vA * uK;
  if (a < 0.004) discard;
  gl_FragColor = vec4(uFoam, a);
}`;

// 粒子（炮口焰 / 硝烟）：点精灵
export const smokeVS = `
attribute float aSize; attribute float aAlpha; attribute vec3 aCol; attribute float aKind; attribute float aSeed;
varying float vAlpha; varying vec3 vCol; varying float vKind; varying float vSeed;
uniform float uScale;
void main(){
  vAlpha = aAlpha; vCol = aCol; vKind = aKind; vSeed = aSeed;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_PointSize = aSize * uScale / max(-mv.z, 0.05);
  gl_Position = projectionMatrix * mv;
}`;
export const smokeFS = `
varying float vAlpha; varying vec3 vCol; varying float vKind; varying float vSeed;
float h2(vec2 p){ return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
float n2(vec2 p){ vec2 i = floor(p), f = fract(p); f = f*f*(3.0-2.0*f);
  return mix(mix(h2(i), h2(i+vec2(1,0)), f.x), mix(h2(i+vec2(0,1)), h2(i+vec2(1,1)), f.x), f.y); }
void main(){
  vec2 p = gl_PointCoord * 2.0 - 1.0; float r = length(p);
  if (r > 1.0) discard;
  if (vKind > 0.5) {               // 炮口焰：亮核 + 柔边（加色）
    float a = pow(1.0 - r, 2.2) * vAlpha;
    gl_FragColor = vec4(vCol * a, a);
  } else {                          // 烟：噪声团
    float n = n2(p * 2.4 + vSeed * 17.0) * 0.6 + n2(p * 5.1 + vSeed * 7.0) * 0.4;
    float a = smoothstep(1.0, 0.25, r + (n - 0.5) * 0.55) * vAlpha;
    gl_FragColor = vec4(vCol * (0.85 + n * 0.3), a);
  }
}`;

// ---------- 船体材质注入 ----------
export const SHIP_COMMON = `
varying vec3 vWPos;
uniform vec3 uCamoCol, uHiColor, uWTint, uWLCol, uWire, uLodCol;
uniform float uCamoOn, uEmisK, uEmisPower, uGlowGain, uChannel, uWL, uHi, uTime, uWLLine, uWater, uCut, uAO, uLodMix;
uniform float uAnimMode, uAnimPower, uAnimSpeed, uAnimOverride, uAnimOvSpeed;
uniform vec2 uCutDir;
uniform float uGameOn, uGameRaw, uSat, uFogDen, uMgSwap, uTmP, uTmS, uAoAmt, uAoFloor, uGoldShift, uPeriShift; uniform vec3 uGain, uToe, uFogCol;
#ifdef HAS_AO
  uniform sampler2D tAO;
#endif
float wmWaterPath(vec3 a, vec3 b){
  float t0 = 0.0, t1 = 1.0; vec3 d = b - a;
  if (abs(d.y) < 1e-6) { if (a.y > uWL) return 0.0; }
  else { float t = (uWL - a.y) / d.y; if (d.y > 0.0) t1 = min(t1, t); else t0 = max(t0, t); }
  if (uCut > 0.5) {
    float sa = dot(a.xz, uCutDir), sd = dot(d.xz, uCutDir);
    if (abs(sd) < 1e-6) { if (sa > 0.0) return 0.0; }
    else { float t = -sa / sd; if (sd > 0.0) t1 = min(t1, t); else t0 = max(t0, t); }
  }
  return max(t1 - t0, 0.0) * length(d);
}
// emissionAnimationMode：1 正弦脉冲 / 2 时间线（animMap，未提供时用阶梯近似）/ 3 滚动遮罩
float wmAnim(float mode, float speed, vec2 uv){
  if (mode < 0.5) return 0.0;
  float t = uTime * speed;
  if (mode < 1.5) return 0.5 + 0.5 * sin(6.2831853 * t);
  if (mode < 2.5) { float k = fract(t * 0.5); return k < 0.1 ? 1.0 : k < 0.2 ? 0.2 : k < 0.3 ? 1.0 : k < 0.75 ? 0.35 + 0.65 * (k - 0.3) / 0.45 : 0.0; }
  return smoothstep(0.0, 0.15, fract(uv.y * 3.0 - t)) * (1.0 - smoothstep(0.35, 0.5, fract(uv.y * 3.0 - t)));
}`;

export const FRAG = {
  map: `
  #ifdef USE_MAP
    vec4 sampledDiffuseColor = texture2D( map, vUv );
    diffuseColor *= sampledDiffuseColor;
  #endif
  vec3 wmAlbRaw = diffuseColor.rgb;
  #ifdef HAS_MG
    vec4 wmMG = texture2D( roughnessMap, vUv );
  #else
    vec4 wmMG = vec4( 0.45, 0.0, 0.0, 1.0 );
  #endif
  #ifdef FAM_PBS
    diffuseColor.rgb = mix( diffuseColor.rgb, uCamoCol, clamp( wmMG.b, 0.0, 1.0 ) * uCamoOn );
  #endif
  vec3 wmAlb = diffuseColor.rgb;`,
  rough: `
  float roughnessFactor = roughness;
  #ifdef HAS_MG
    roughnessFactor = clamp( 1.0 - mix( wmMG.r, wmMG.g, uMgSwap ), 0.07, 1.0 );
  #endif`,
  metal: `
  float metalnessFactor = metalness;
  #ifdef HAS_MG
    metalnessFactor = mix( wmMG.g, wmMG.r, uMgSwap );
  #endif`,
  normal: `
  #ifdef USE_NORMALMAP
    vec2 wmNxy = texture2D( normalMap, vUv ).rg * 2.0 - 1.0;
    vec3 mapN = vec3( wmNxy, sqrt( clamp( 1.0 - dot( wmNxy, wmNxy ), 0.0, 1.0 ) ) );
    mapN.xy *= normalScale;
    normal = perturbNormal2Arb( - vViewPosition, normal, mapN, faceDirection );
  #endif`,
  emis: `
  #ifdef FAM_EMISSIVE
    float wmMode = uAnimOverride > -0.5 ? uAnimOverride : uAnimMode;
    float wmSpd = uAnimOverride > -0.5 ? uAnimOvSpeed : uAnimSpeed;
    float wmA = wmAnim( wmMode, wmSpd, vUv );
    float wmP = wmMode > 0.5 ? uEmisPower + uAnimPower * wmA : uEmisPower;
    totalEmissiveRadiance += wmMG.b * wmMG.b * wmAlbRaw * wmP * uEmisK * uGlowGain;
  #endif`,
  ao: `
  #ifdef HAS_AO
    float ambientOcclusion = mix( 1.0, texture2D( tAO, vUv ).r, uAO );
    reflectedLight.indirectDiffuse *= ambientOcclusion;
    reflectedLight.directDiffuse *= mix( 1.0, ambientOcclusion, 0.35 );
    #if defined( USE_ENVMAP ) && defined( STANDARD )
      float dotNV = saturate( dot( geometry.normal, geometry.viewDir ) );
      reflectedLight.indirectSpecular *= computeSpecularOcclusion( dotNV, ambientOcclusion, material.roughness );
    #endif
  #endif`,
  post: `
  gl_FragColor.rgb = clamp( gl_FragColor.rgb, 0.0, 64.0 );
  // 游戏对齐(港口实拍拟合, 见 CALIBRATION.md): 逐通道曝光/白平衡 uGain → 暗部 toe → 压缩型色调曲线 x^p/(x^p+1)
  // → 饱和度 → 远距离空气透视。输出已是显示线性值(合成时不再做 Neutral 色调映射)。uGameRaw>0: 拟合用, 直接输出辐亮度×uGameRaw。
  if ( uGameOn > 0.5 && uChannel < 0.5 ) {
    // 近似接触 AO(补港口 SSAO): 天光可见度 + 底面压暗 + 掠射 + 水线附近接触阴影
    if ( uAoAmt > 0.001 ) {
      vec3 wn = normalize( normal );
      float ndu = saturate( wn.y * 0.5 + 0.5 );
      float under = smoothstep( 0.2, -0.5, wn.y );
      float ndv = saturate( abs( dot( wn, normalize( vViewPosition ) ) ) );
      float nearWl = exp( -max( vWPos.y - uWL, 0.0 ) * 3.5 ) * smoothstep( 0.5, -0.2, wn.y );
      float ao = mix( uAoFloor, 1.0, ndu );
      ao *= mix( mix( uAoFloor, 1.0, 0.55 ), 1.0, pow( ndv, 0.4 ) );
      ao *= ( 1.0 - 0.35 * under ) * ( 1.0 - 0.4 * nearWl );
      gl_FragColor.rgb *= mix( 1.0, clamp( ao, uAoFloor * 0.85, 1.0 ), uAoAmt );
    }
    vec3 wmPreG = gl_FragColor.rgb;
    if ( uGameRaw > 0.0 ) gl_FragColor.rgb = gl_FragColor.rgb * uGameRaw;
    else {
      vec3 gx = max( gl_FragColor.rgb * uGain - uToe, 0.0 );
      vec3 gp = pow( gx, vec3( uTmP ) );
      vec3 gc = min( uTmS * gp / ( gp + 1.0 ), vec3( 1.0 ) );
      float wmGL = dot( gc, vec3( 0.2126, 0.7152, 0.0722 ) );
      // 发光不参与夜间去饱和: 按自发光占像素辐亮度的比例把饱和度拉回 ≥1(港口夜景发光线/道具发光保持原色)
      float wmEF = clamp( dot( totalEmissiveRadiance, vec3( 0.2126, 0.7152, 0.0722 ) ) / max( dot( wmPreG, vec3( 0.2126, 0.7152, 0.0722 ) ), 1e-4 ), 0.0, 1.0 );
      float wmSat = mix( uSat, max( uSat, 1.0 ), smoothstep( 0.05, 0.6, wmEF ) );
      gc = clamp( mix( vec3( wmGL ), gc, wmSat ), 0.0, 1.0 );
      // 金色色相补偿: 港口金偏黄/橄榄, 工作台偏橙 → 高彩暖色抬 G、略压 R
      if ( uGoldShift > 0.001 ) {
        float mx = max( gc.r, max( gc.g, gc.b ) ), mn = min( gc.r, min( gc.g, gc.b ) );
        float ch = mx - mn;
        float warm = step( gc.b + 0.02, gc.r ) * step( gc.b * 0.85, gc.g );
        float k = warm * smoothstep( 0.06, 0.22, ch ) * uGoldShift;
        gc.g = min( 1.0, gc.g + ( gc.r - gc.g ) * 0.55 * k );
        gc.r = max( 0.0, gc.r * ( 1.0 - 0.18 * k ) );
        gc.b = max( 0.0, gc.b * ( 1.0 - 0.08 * k ) );
      }
      // peri/royal: 港口深紫更红; 正 periHue 把青紫旋向品红
      if ( abs( uPeriShift ) > 0.001 ) {
        float mx2 = max( gc.r, max( gc.g, gc.b ) ), mn2 = min( gc.r, min( gc.g, gc.b ) );
        float ch2 = mx2 - mn2;
        float bluish = step( gc.r + 0.02, gc.b ) * step( 0.08, ch2 );
        float pk = bluish * smoothstep( 0.08, 0.28, ch2 ) * clamp( uPeriShift, -1.0, 1.0 );
        gc.r = clamp( gc.r + 0.22 * pk, 0.0, 1.0 );
        gc.b = clamp( gc.b - 0.10 * pk, 0.0, 1.0 );
      }
      float fd = length( vWPos - cameraPosition );
      gl_FragColor.rgb = mix( gc, uFogCol, 1.0 - exp( -fd * uFogDen ) );
    }
  }
  if ( uWater > 0.5 && uChannel < 0.5 ) {
    float L = wmWaterPath( cameraPosition, vWPos );
    float dep = max( uWL - vWPos.y, 0.0 );
    vec2 cq = vWPos.xz * 3.1 + vec2( uTime*0.35, uTime*0.22 );
    float cau = pow( abs( sin( cq.x + sin( cq.y*1.3 ) ) * sin( cq.y*0.9 + sin( cq.x*1.1 + uTime*0.4 ) ) ), 3.0 );
    gl_FragColor.rgb *= 1.0 + 0.35 * cau * exp( -dep*2.5 ) * step( 0.001, dep );
    vec3 tr = exp( -L * vec3( 0.42, 0.13, 0.10 ) );
    gl_FragColor.rgb = gl_FragColor.rgb * tr + uWTint * ( 1.0 - exp( -L*0.35 ) ) * 0.18;
    // 水线接触(1 单位≈15 m): 水下随深度快速沉入水体色; 水线上 ~1.5 m 湿带压暗; 水线处细白浪线(亮度随环境光)
    float wmH = vWPos.y - uWL;
    float wmSink = ( 1.0 - exp( -dep * 14.0 ) ) * step( 0.001, dep );
    gl_FragColor.rgb = mix( gl_FragColor.rgb, uWTint * 0.55, wmSink * 0.85 );
    float wmWet = exp( -max( wmH, 0.0 ) * 18.0 ) * step( 0.0, wmH );
    gl_FragColor.rgb *= 1.0 - 0.28 * wmWet;
    float wmFoam = 1.0 - smoothstep( 0.015, 0.06, abs( wmH - 0.015 ) );
    float wmAmb = uGameOn > 0.5 ? clamp( dot( uFogCol, vec3( 0.33 ) ) * 1.4, 0.08, 1.0 ) : 0.9;
    gl_FragColor.rgb = mix( gl_FragColor.rgb, vec3( 0.86, 0.9, 0.92 ) * wmAmb, wmFoam * 0.55 );
  }
  if ( uWLLine > 0.5 && uChannel < 0.5 ) {
    float wd = abs( vWPos.y - uWL ) / max( fwidth( vWPos.y ), 1e-5 );
    gl_FragColor.rgb = mix( gl_FragColor.rgb, uWLCol, ( 1.0 - smoothstep( 0.6, 1.6, wd ) ) * 0.9 );
  }
  if ( uChannel > 0.5 && uChannel < 1.5 ) gl_FragColor = vec4( wmAlb, 1.0 );
  else if ( uChannel > 1.5 && uChannel < 2.5 ) gl_FragColor = vec4( pow( normalize( normal ) * 0.5 + 0.5, vec3( 2.2 ) ), 1.0 );
  else if ( uChannel > 2.5 && uChannel < 3.5 ) gl_FragColor = vec4( gl_FragColor.rgb * 0.1 + totalEmissiveRadiance, 1.0 );
  else if ( uChannel > 3.5 && uChannel < 4.5 ) gl_FragColor = vec4( uWire, 1.0 );
  else if ( uChannel > 4.5 ) {
    float mk = clamp( wmMG.b, 0.0, 1.0 );
    #ifdef FAM_EMISSIVE
      vec3 mc = vec3( 0.05, 0.55, 1.0 );
    #else
      vec3 mc = vec3( 1.0, 0.36, 0.06 );
    #endif
    gl_FragColor = vec4( mix( vec3( dot( wmAlbRaw, vec3( 0.3, 0.59, 0.11 ) ) * 0.35 + 0.05 ), mc, mk ), 1.0 );
  }
  if ( uLodMix > 0.001 ) gl_FragColor.rgb = mix( gl_FragColor.rgb, uLodCol * ( 0.35 + 0.65 * dot( wmAlb, vec3( 0.33 ) ) ), uLodMix );
  if ( uHi > 0.001 ) {
    float rim = pow( 1.0 - abs( dot( normalize( normal ), normalize( vViewPosition ) ) ), 3.0 );
    gl_FragColor.rgb = mix( gl_FragColor.rgb, uHiColor * ( 0.6 + 0.4 * rim ), clamp( uHi * ( 0.12 + rim * 1.5 ), 0.0, 0.8 ) );
  }`,
};

// 合成：Khronos PBR Neutral 色调映射（比 ACES 更保色相，深蓝不会被推成灰褐）+ sRGB + 对比模式
export const compFS = `
uniform sampler2D tA, tB; uniform float uMode, uOverlay, uRaw, uExposure, uVig, uDither; varying vec2 vUv;
vec3 neutral(vec3 c){
  c *= uExposure;
  const float startCompression = 0.8 - 0.04; const float desaturation = 0.15;
  float x = min(c.r, min(c.g, c.b)); float offset = x < 0.08 ? x - 6.25 * x * x : 0.04; c -= offset;
  float peak = max(c.r, max(c.g, c.b));
  if (peak < startCompression) return c;
  const float d = 1.0 - startCompression; float newPeak = 1.0 - d * d / (peak + d - startCompression);
  c *= newPeak / peak; float g = 1.0 - 1.0 / (desaturation * (peak - newPeak) + 1.0);
  return mix(c, vec3(newPeak), g);
}
vec3 srgb(vec3 c){ c = clamp(c, 0.0, 1.0); return mix(c*12.92, 1.055*pow(c, vec3(1.0/2.4)) - 0.055, step(0.0031308, c)); }
vec3 tm(vec3 c){ return uRaw > 0.5 ? srgb(c) : srgb(neutral(c)); }
void main(){
  vec3 col = tm(texture2D(tB, vUv).rgb);
  if (uMode > 2.5) col = mix(col, tm(texture2D(tA, vUv).rgb), uOverlay);
  float v = smoothstep(0.95, 0.25, length((vUv - 0.5) * vec2(1.0, 0.8)));
  col *= mix(1.0, v, uVig);
  col += uDither * (fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
  gl_FragColor = vec4(col, 1.0);
}`;
