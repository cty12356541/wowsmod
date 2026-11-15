"""Refit Robin's FGM049 armor and MidBack funnel against current project exports.

Run with Blender in background mode:
    blender -b --factory-startup -P refine_geometry.py -- turret SOURCE.glb PATCH.glb
    blender -b --factory-startup -P refine_geometry.py -- funnel SOURCE.glb PATCH.glb

SOURCE is the corresponding wowsmod model export-blender GLB. The output only
contains the edited rendering set and its existing parent. A .blend next to it
also contains the original neighboring meshes for checking fit and clearance.
Coordinates below use Blender axes, with Z up and Y forward.
"""

import argparse
import math
from pathlib import Path
import sys

import bmesh
import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree


WHITE = (0.3125, 0.5)
LAVENDER = (0.4375, 0.5)
VIOLET = (0.5625, 0.5)
GOLD = (0.8125, 0.5)
LUMINOUS = (0.9375, 0.5)


class Surface:
    def __init__(self, obj):
        self.tree = BVHTree.FromPolygons(
            [obj.matrix_world @ v.co for v in obj.data.vertices],
            [tuple(p.vertices) for p in obj.data.polygons],
        )

    def hit(self, start, direction, distance=1.0):
        point, _, _, _ = self.tree.ray_cast(Vector(start), Vector(direction), distance)
        if point is None:
            raise ValueError(f"Missing reference surface at {start} in direction {direction}")
        return point


class Draft:
    def __init__(self):
        self.mesh = bmesh.new()
        self.uv = self.mesh.loops.layers.uv.new("UVMap")

    def face(self, vertices, uv, smooth=False):
        face = self.mesh.faces.new(vertices)
        face.smooth = smooth
        for loop, (u, v) in zip(face.loops, uv):
            loop[self.uv].uv = (u, 1 - v)

    def loft(self, rows, paint, *, closed=False, smooth=False):
        """Sweep closed polygonal sections through a path, optionally closing the path."""
        rows = [[self.mesh.verts.new(co) for co in row] for row in rows]
        corners = len(rows[0])
        steps = len(rows) if closed else len(rows) - 1
        for i in range(steps):
            first, second = rows[i], rows[(i + 1) % len(rows)]
            for j in range(corners):
                k = (j + 1) % corners
                self.face(
                    [first[j], second[j], second[k], first[k]],
                    paint(i / steps, (i + 1) / steps, j),
                    smooth,
                )
        if not closed:
            self.face(rows[0][::-1], [paint(0, 0, 0)[0]] * corners)
            self.face(rows[-1], [paint(1, 1, 0)[0]] * corners)

    def retain(self, source, select, transform=lambda point, _uv: point):
        """Keep smaller ornaments from the current mesh with their current UVs."""
        saved = {}
        uv_map = source.data.uv_layers.active.data
        for polygon in source.data.polygons:
            points = [source.matrix_world @ source.data.vertices[i].co for i in polygon.vertices]
            uv = [(uv_map[i].uv.x, 1 - uv_map[i].uv.y) for i in polygon.loop_indices]
            if not select(points, uv):
                continue
            vertices = []
            for i, point, color in zip(polygon.vertices, points, uv):
                if i not in saved:
                    saved[i] = self.mesh.verts.new(transform(point, color))
                vertices.append(saved[i])
            self.face(vertices, uv, polygon.use_smooth)
        if not saved:
            raise ValueError(f"Could not find ornaments in {source.name}")

    def finish(self, obj, *, turret_atlas=False):
        bmesh.ops.remove_doubles(self.mesh, verts=self.mesh.verts, dist=1e-6)
        bmesh.ops.recalc_face_normals(self.mesh, faces=self.mesh.faces)
        if turret_atlas:
            unpin_turret_atlas_faces(self.mesh, self.uv)
        bmesh.ops.triangulate(self.mesh, faces=self.mesh.faces)
        for edge in self.mesh.edges:
            if edge.is_manifold:
                edge.smooth = edge.calc_face_angle() < math.radians(27)
        if any(f.calc_area() < 1e-10 for f in self.mesh.faces):
            raise ValueError(f"Degenerate triangles in {obj.name}")
        mesh = bpy.data.meshes.new(obj.data.name + "_refined")
        self.mesh.to_mesh(mesh)
        self.mesh.free()
        mesh.transform(obj.matrix_world.inverted())
        mesh.materials.append(obj.data.materials[0])
        obj.data = mesh


def unpin_turret_atlas_faces(mesh, uv_layer):
    """Lay out painted bevels and ornaments inside flat, correctly lit atlas areas."""
    mesh.normal_update()
    for face in mesh.faces:
        sample = face.loops[0][uv_layer].uv.copy()
        if any((loop[uv_layer].uv - sample).length > 1e-6 for loop in face.loops):
            continue

        # The atlas uses black AO at v=.5 in the white/lavender panel columns.
        # Their front paint band has clean AO and no rivets or dark seams.
        painted = abs(sample.x - WHITE[0]) < 0.001 or abs(sample.x - LAVENDER[0]) < 0.001
        center_v = 1 - 0.135 if painted else sample.y

        # Project through the strongest component of the face normal so every
        # painted facet has a real UV area for tangent-space lighting.
        axis = max(range(3), key=lambda i: abs(face.normal[i]))
        axes = [i for i in range(3) if i != axis]
        coords = [tuple(loop.vert.co[i] for i in axes) for loop in face.loops]
        lo = [min(values) for values in zip(*coords)]
        hi = [max(values) for values in zip(*coords)]
        for loop, (x, y) in zip(face.loops, coords):
            u = (x - lo[0]) / (hi[0] - lo[0]) - 0.5
            v = (y - lo[1]) / (hi[1] - lo[1]) - 0.5
            loop[uv_layer].uv = (sample.x + 0.032 * u, center_v + 0.04 * v)


def solid(color):
    return lambda _u0, _u1, _j: [color] * 4


def strip_uv(u0, u1, v0, v1):
    return [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]


def roof_plates(draft, armor):
    def level(x, y):
        return armor.hit((x, y, 0.4), (0, 0, -1), 0.25).z

    for sign in (-1, 1):
        # Leave the longitudinal center stripe exposed. The aft access plate
        # carries the original star and the forward plate follows the roof slope.
        for aft, lo, hi, halfwidth in [(True, -0.515, -0.400, 0.233), (False, -0.394, -0.178, 0.252)]:
            front = [(-0.221, 0.0), (-0.200, 0.0), (-0.190, 0.0)] if not aft else []
            stations = [(lo, 0.018), (lo + 0.008, 0.005), (lo + 0.020, 0.0)]
            stations += front + [(hi - 0.006, 0.007), (hi, 0.014)]
            rows = []
            for i, (y, cut) in enumerate(stations):
                a, b = 0.023 + cut * 0.25, halfwidth - cut
                edge = i in (0, len(stations) - 1)
                # Thin lip below the visible bevel and a shallow crown above it.
                h = 0.0028 if edge else (0.0058 if aft else 0.0064)
                section = [
                    (a + 0.004, -0.0014), (b - 0.004, -0.0014),
                    (b, 0.0011), (b - 0.0018, h * 0.68), (b - 0.0066, h),
                    (a + 0.0060, h), (a + 0.0016, h * 0.68), (a, 0.0011),
                ]
                rows.append([(sign * x, y, level(sign * x, y) + z) for x, z in section])

            def paint(t0, t1, j):
                # Sample the paint inside the same atlas columns as the source;
                # physical seams now supply the dark lines previously in the map.
                if j == 4:
                    u0, u1 = (0.415, 0.462) if aft else (0.286, 0.339)
                    return [(u1, 0.103 + 0.071 * t0), (u1, 0.103 + 0.071 * t1),
                            (u0, 0.103 + 0.071 * t1), (u0, 0.103 + 0.071 * t0)]
                return [LAVENDER if aft or j in (1, 7) else WHITE] * 4

            draft.loft(rows, paint)


def shoulder_plates(draft):
    # The main armor cross-section sampled from Turret at Z .075-.137.
    # Keep clear of the wider rangefinders and gun ports used in the old outline.
    right = [(0.0, -0.60224), (0.3367, -0.5701), (0.41028, -0.2343),
             (0.41028, 0.0771), (0.3923, 0.1605)]
    outline = [Vector((-x, y)) for x, y in reversed(right[1:])] + [Vector(p) for p in right]
    lengths = [0.0]
    for start, end in zip(outline, outline[1:]):
        lengths.append(lengths[-1] + (end - start).length)

    def station(t):
        j = next(i for i in range(len(lengths) - 1) if t <= lengths[i + 1] + 1e-7)
        direction = (outline[j + 1] - outline[j]).normalized()
        point = outline[j] + direction * (t - lengths[j])
        normal = Vector((direction.y, -direction.x))
        # Angle bisectors keep the sweep watertight around actual armor corners.
        if j + 2 < len(outline) and abs(t - lengths[j + 1]) < 1e-6:
            other = (outline[j + 2] - outline[j + 1]).normalized()
            average = (normal + Vector((other.y, -other.x))).normalized()
            normal = average / average.dot(normal)
        return point, normal

    def relief(path, profile, floor=0.073, ceiling=0.142):
        rows = []
        for t in path:
            point, normal = station(t)
            # Rangefinder cans occupy the outer aft shoulders above this taper.
            top = ceiling - 0.013 * max(0, min(1, (-point.y - 0.34) / 0.16))
            row = []
            for offset, height in profile:
                xy = point + offset * normal
                row.append((xy.x, xy.y, floor + height * (top - floor)))
            rows.append(row)
        return rows

    def steps(a, b, inset):
        return sorted({a, a + inset, b - inset, b, *(t for t in lengths if a < t < b)})

    total = lengths[-1]
    backing = [(-0.004, 0.12), (0.0028, 0.17), (0.0043, 0.28),
               (0.0043, 0.76), (0.0028, 0.86), (-0.004, 0.9)]
    draft.loft(relief(steps(0.004, total - 0.004, 0.014), backing), solid(LAVENDER))

    panel_edges = [0, 0.145, 0.284, 0.432, 0.568, 0.716, 0.855, 1]
    colors = [WHITE, LAVENDER, WHITE, LAVENDER, WHITE, LAVENDER, WHITE]
    for i, (f0, f1) in enumerate(zip(panel_edges, panel_edges[1:])):
        begin, end = total * f0 + 0.0026, total * f1 - 0.0026
        path = steps(begin, end, 0.007)
        profile = [(-0.004, 0.0), (0.0015, 0.00), (0.0088, 0.12), (0.012, 0.19),
                   (0.012, 0.74), (0.010, 0.86), (0.0035, 0.98), (-0.004, 1.00)]
        rows = relief(path, profile)
        for index in (0, -1):
            point, _ = station(path[index])
            rows[index] = [(0.52 * x + 0.48 * point.x, 0.52 * y + 0.48 * point.y, z)
                           for x, y, z in rows[index]]

        def paint(t0, t1, j):
            color = colors[i]
            if j == 3:
                return strip_uv(color[0] - 0.019 + 0.038 * t0, color[0] - 0.019 + 0.038 * t1,
                                0.157, 0.103)
            return [color if j in (4, 5) else LAVENDER] * 4

        draft.loft(rows, paint)

    # Two short flush seam keys at the center of the forward shoulder panels.
    # Their thin cyan face sits between the gold shoulder and the armor bevel.
    for start, end in [(0.049, 0.097), (0.903, 0.951)]:
        path = steps(total * start, total * end, 0.006)
        inset = [(0.008, 0.68), (0.0125, 0.69), (0.0132, 0.71),
                 (0.0132, 0.73), (0.0125, 0.75), (0.008, 0.76)]
        rows = relief(path, inset)
        draft.loft(rows, lambda _t0, _t1, j: [LUMINOUS if j == 2 else GOLD] * 4)


def refine_turret(obj):
    armor = Surface(bpy.data.objects["Turret"])
    draft = Draft()
    roof_plates(draft, armor)
    shoulder_plates(draft)

    def retain_star(points, uv):
        return (all(abs(u - GOLD[0]) < 0.001 for u, _ in uv) and
                (min(p.z for p in points) > 0.246 or max(p.y for p in points) < -0.615))

    def seat_star(point, _uv):
        if point.z > 0.246:
            center = Vector((math.copysign(0.144, point.x), -0.4442, 0.2555))
            return (point - center) * 0.67 + Vector((center.x, -0.454, 0.2500))
        return (0.62 * point.x, -0.609 + (point.y + 0.629) * 0.42 + 0.095 * abs(point.x),
                0.104 + 0.56 * (point.z - 0.09075))

    draft.retain(obj, retain_star, seat_star)
    draft.finish(obj, turret_atlas=True)


def funnel_xy(theta, width, length):
    c, s = math.cos(theta), math.sin(theta)
    r = (abs(c / length) ** 3 + abs(s / width) ** 3) ** (-1 / 3)
    return (r * s, -1.72 + r * c)


def funnel_crown(draft):
    # One closed sheet-metal profile with a rolled gold cap, return underneath,
    # and a recessed fine seam. Each entry defines the color toward the next row.
    profile = [
        (0.375, 0.557, 1.823, LAVENDER), (0.376, 0.558, 1.840, GOLD),
        (0.383, 0.565, 1.849, GOLD), (0.399, 0.581, 1.849, GOLD),
        (0.409, 0.591, 1.844, GOLD), (0.412, 0.594, 1.836, GOLD),
        (0.409, 0.591, 1.829, WHITE), (0.402, 0.584, 1.820, LAVENDER),
        (0.416, 0.603, 1.778, LAVENDER), (0.416, 0.605, 1.764, VIOLET),
        (0.412, 0.601, 1.761, LUMINOUS), (0.413, 0.602, 1.757, VIOLET),
        (0.419, 0.608, 1.752, LAVENDER), (0.427, 0.617, 1.711, WHITE),
        (0.427, 0.617, 1.702, GOLD), (0.440, 0.629, 1.696, GOLD),
        (0.444, 0.633, 1.688, GOLD), (0.440, 0.629, 1.681, GOLD),
        (0.428, 0.617, 1.676, LAVENDER), (0.392, 0.576, 1.684, VIOLET),
        (0.366, 0.548, 1.696, LAVENDER),
    ]
    segments = 96
    rows = []
    for i in range(segments):
        theta = 2 * math.pi * i / segments
        # Shallow facets mark the pressed panels without duplicating the trim.
        facet = 0.0025 * (1 - math.cos(theta * 8)) * 0.5
        rows.append([(*funnel_xy(theta, width - facet, length - facet), height)
                     for width, length, height, _ in profile])
    draft.loft(rows, lambda _t0, _t1, j: [profile[j][3]] * 4, closed=True, smooth=True)


def funnel_saddles(draft, chimney):
    for sign in (-1, 1):
        for y, base_z in [(-2.115, 1.521), (-1.890, 1.518)]:
            wall = chimney.hit((sign * 0.46, y, base_z), (-sign, 0, 0)).x
            y_out = y + (-0.035 if y < -2 else 0.054)
            tip_x = 0.404 * (1 - abs((y_out + 1.72) / 0.591) ** 3) ** (1 / 3)
            stations = [(wall - sign * 0.006, y, base_z + 0.014, 0.055, 0.041),
                        (wall + sign * 0.010, y, base_z + 0.022, 0.053, 0.041),
                        (sign * 0.229, y, base_z + 0.066, 0.041, 0.036),
                        (sign * 0.278, (y + y_out) / 2, 1.632, 0.037, 0.030),
                        (sign * (tip_x - 0.040), y_out, 1.667, 0.041, 0.020),
                        (sign * tip_x, y_out, 1.687, 0.052, 0.013)]
            rows = []
            for x, cy, z, w, h in stations:
                cross = [(-0.78, -1), (0.78, -1), (1, -0.65), (1, 0.65),
                         (0.78, 1), (-0.78, 1), (-1, 0.65), (-1, -0.65)]
                rows.append([(x, cy + u * w, z + v * h) for u, v in cross])
            draft.loft(rows, lambda t0, _t1, j: [GOLD if t0 > 0.7 and j in (2, 3, 5, 6)
                                                   else WHITE if j in (2, 3, 4, 5, 6) else LAVENDER] * 4)


def funnel_streamers(draft):
    # Sew the hem into the folded sheets and end the old long gold threads
    # before they project over the nearby gun positions.
    for sign in (-1, 1):
        rows = []
        for i in range(17):
            t = i / 16
            x = sign * (0.105 + 0.192 * t + 0.014 * math.sin(math.pi * t))
            y = -2.299 - 0.564 * t
            z = 1.698 - 0.314 * t ** 1.12
            w = 0.058 * (1 - t ** 2) + 0.002
            h = 0.005 * (1 - 0.76 * t)
            roll = sign * (0.11 + 0.56 * t)
            cross = [(-0.8, -1), (0.8, -1), (1, -0.52), (1, 0.52),
                     (0.8, 1), (-0.8, 1), (-1, 0.52), (-1, -0.52)]
            rows.append([(x + u * w * math.cos(roll) - v * h * math.sin(roll), y,
                          z + u * w * math.sin(roll) + v * h * math.cos(roll)) for u, v in cross])
        draft.loft(rows, lambda _t0, _t1, j: [GOLD if j in (2, 6) else LAVENDER] * 4,
                   smooth=True)


def refine_funnel(obj):
    chimney = Surface(bpy.data.objects["MidBack_DeckHouse_Chimney"])
    draft = Draft()
    funnel_crown(draft)
    funnel_saddles(draft, chimney)
    funnel_streamers(draft)

    def retain_knot(points, uv):
        return all(0.138 < u < 0.237 for u, _ in uv) or (
            all(abs(u - GOLD[0]) < 0.001 for u, _ in uv) and
            all(abs(p.x) < 0.051 and -1.131 < p.y < -1.029 and 1.709 < p.z < 1.811 for p in points)
        )

    draft.retain(obj, retain_knot)
    draft.finish(obj)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("asset", choices=("turret", "funnel"))
    parser.add_argument("source", type=Path)
    parser.add_argument("patch", type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:])

    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(args.source.resolve()))
    bpy.context.view_layer.update()
    if args.asset == "turret":
        name, parent, refine = "rb_shell_FGM049", "part|FGM049_380mm_45_Mle_1935", refine_turret
    else:
        name, parent, refine = "rb_funnel", "section|MidBack", refine_funnel
    obj = bpy.data.objects[name]
    if obj.parent.name != parent or obj.get("wowsmod_renderset") != name:
        raise ValueError(f"{args.source}: expected the original rendering set and parent")
    before = len(obj.data.polygons)
    refine(obj)

    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    obj.parent.select_set(True)
    bpy.context.view_layer.objects.active = obj
    patch = args.patch.resolve()
    patch.parent.mkdir(parents=True, exist_ok=True)
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(patch.with_suffix(".blend")))
    bpy.ops.export_scene.gltf(filepath=str(patch), export_format="GLB", use_selection=True,
                             export_extras=True, export_normals=True)
    print(f"{name}: {before} -> {len(obj.data.polygons)} triangles")


if __name__ == "__main__":
    main()
