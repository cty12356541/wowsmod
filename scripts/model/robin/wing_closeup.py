# 翼部特写(仅外带翼, 工作台): blender -b -P wing_closeup.py -- <wing_R.glb> <atlas.png> <out_prefix>
import bpy, sys, math
from mathutils import Vector
a=sys.argv[sys.argv.index('--')+1:]; G,ATL,OUT=a
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=G)
m=bpy.data.materials.new('atl'); m.use_nodes=True; nt=m.node_tree; t=nt.nodes.new('ShaderNodeTexImage'); t.image=bpy.data.images.load(ATL)
nt.links.new(t.outputs['Color'],nt.nodes['Principled BSDF'].inputs['Base Color']); nt.nodes.active=t
for o in bpy.context.scene.objects:
    if o.type=='MESH':
        o.data.materials.clear(); o.data.materials.append(m)
s=bpy.context.scene; s.render.engine='BLENDER_WORKBENCH'; s.display.shading.light='STUDIO'; s.display.shading.color_type='TEXTURE'
s.display.shading.show_cavity=True; s.render.resolution_x,s.render.resolution_y=800,600
w=bpy.data.worlds.new('w'); s.world=w; w.color=(0.2,0.3,0.36)
cam=bpy.data.objects.new('c',bpy.data.cameras.new('c')); s.collection.objects.link(cam); s.camera=cam
tgt=Vector((1.6,-1.1,1.9))
for k,loc,lens in [('aft',(1.6,-7.5,1.9),55),('top',(2.4,-0.6,7.0),50),('stbd',(8.0,-1.0,2.2),45)]:
    cam.location=loc; cam.data.lens=lens; cam.rotation_euler=(tgt-Vector(loc)).to_track_quat('-Z','Y').to_euler()
    s.render.filepath=f'{OUT}_{k}.png'; bpy.ops.render.render(write_still=True)
