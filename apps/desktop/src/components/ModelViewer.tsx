import { useEffect, useRef, useState } from 'react'
import * as THREE from 'three'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OBJLoader } from 'three/examples/jsm/loaders/OBJLoader.js'
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js'
import { PLYLoader } from 'three/examples/jsm/loaders/PLYLoader.js'
import { ThreeMFLoader } from 'three/examples/jsm/loaders/3MFLoader.js'

interface Props {
  /** A URL the webview can load (see `assetUrl`). */
  url: string
  ext: string
  /** Bumped to reload after the file changed on disk. */
  version?: number
}

async function load(url: string, ext: string): Promise<THREE.Object3D> {
  switch (ext) {
    case 'glb':
    case 'gltf': {
      const gltf = await new GLTFLoader().loadAsync(url)
      return gltf.scene
    }
    case 'obj':
      return new OBJLoader().loadAsync(url)
    case 'stl': {
      const geom = await new STLLoader().loadAsync(url)
      geom.computeVertexNormals()
      return new THREE.Mesh(geom, new THREE.MeshStandardMaterial({ color: 0x9aa7b8, metalness: 0.1, roughness: 0.7, side: THREE.DoubleSide }))
    }
    case 'ply': {
      const geom = await new PLYLoader().loadAsync(url)
      if (!geom.getAttribute('normal')) geom.computeVertexNormals()
      const hasColor = !!geom.getAttribute('color')
      return new THREE.Mesh(geom, new THREE.MeshStandardMaterial({ vertexColors: hasColor, color: hasColor ? 0xffffff : 0x9aa7b8, roughness: 0.7, side: THREE.DoubleSide }))
    }
    case '3mf':
      return new ThreeMFLoader().loadAsync(url)
    default:
      throw new Error(`No viewer for .${ext}`)
  }
}

/**
 * Orbitable 3D preview (three.js) for glTF/GLB, OBJ, STL, PLY and 3MF.
 * Drag to orbit, wheel to zoom, right-drag to pan. Loaded lazily by the
 * asset view so the main bundle stays small.
 */
export default function ModelViewer({ url, ext, version = 0 }: Props) {
  const host = useRef<HTMLDivElement>(null)
  const [error, setError] = useState<string | null>(null)
  const [info, setInfo] = useState<string>('')

  useEffect(() => {
    const el = host.current
    if (!el) return
    setError(null)
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    el.appendChild(renderer.domElement)
    const scene = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1e6)
    scene.add(new THREE.HemisphereLight(0xffffff, 0x667788, 1.2))
    const key = new THREE.DirectionalLight(0xffffff, 1.4)
    key.position.set(1, 2, 3)
    scene.add(key)
    const fill = new THREE.DirectionalLight(0xffffff, 0.5)
    fill.position.set(-2, -1, -1)
    scene.add(fill)
    const controls = new OrbitControls(camera, renderer.domElement)
    controls.enableDamping = true
    let frame = 0
    let disposed = false
    const resize = () => {
      const w = el.clientWidth || 1
      const h = el.clientHeight || 1
      renderer.setSize(w, h, false)
      camera.aspect = w / h
      camera.updateProjectionMatrix()
    }
    const ro = new ResizeObserver(resize)
    ro.observe(el)
    resize()
    const tick = () => {
      if (disposed) return
      controls.update()
      renderer.render(scene, camera)
      frame = requestAnimationFrame(tick)
    }
    tick()

    let grid: THREE.GridHelper | null = null
    load(url, ext)
      .then((obj) => {
        if (disposed) return
        scene.add(obj)
        const box = new THREE.Box3().setFromObject(obj)
        const size = box.getSize(new THREE.Vector3())
        const centre = box.getCenter(new THREE.Vector3())
        const radius = Math.max(size.length() / 2, 1e-3)
        // STL/3MF are usually Z-up; glTF is Y-up. Show the model as authored and fit the camera.
        camera.position.copy(centre).add(new THREE.Vector3(radius * 1.2, radius * 0.9, radius * 1.8))
        camera.near = radius / 100
        camera.far = radius * 100
        camera.updateProjectionMatrix()
        controls.target.copy(centre)
        controls.update()
        grid = new THREE.GridHelper(radius * 4, 20, 0x8899aa, 0xdde3ea)
        grid.position.set(centre.x, box.min.y, centre.z)
        scene.add(grid)
        let tris = 0
        obj.traverse((o) => {
          const mesh = o as THREE.Mesh
          if (mesh.isMesh && mesh.geometry) {
            const g = mesh.geometry
            tris += (g.index ? g.index.count : g.getAttribute('position')?.count ?? 0) / 3
          }
        })
        const fmt = (n: number) => String(Math.round(n * 100) / 100)
        setInfo(`${fmt(size.x)} × ${fmt(size.y)} × ${fmt(size.z)} · ${Math.round(tris).toLocaleString()} triangles`)
      })
      .catch((e) => {
        if (!disposed) setError(String(e?.message ?? e))
      })

    return () => {
      disposed = true
      cancelAnimationFrame(frame)
      ro.disconnect()
      controls.dispose()
      scene.traverse((o) => {
        const mesh = o as THREE.Mesh
        if (mesh.isMesh) {
          mesh.geometry?.dispose()
          const m = mesh.material
          if (Array.isArray(m)) m.forEach((x) => x.dispose())
          else m?.dispose()
        }
      })
      renderer.dispose()
      renderer.domElement.remove()
    }
  }, [url, ext, version])

  return (
    <div className="model-viewer">
      <div ref={host} className="model-viewer__canvas" aria-label="3D model" />
      {info && !error && <div className="model-viewer__info">{info} · drag to orbit, wheel to zoom</div>}
      {error && <div className="model-viewer__error">Could not load the model: {error}</div>}
    </div>
  )
}
