"""Rebuild original Whatever starter content using Python's standard library.

Existing Noto Sans and its OFL are preserved. All generated content is covered by
the repository license. No downloaded models, textures, audio or runtime code.
"""
from pathlib import Path
import json
import math
import random
import struct
import wave
import zlib

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "engine/resources/starter"


def write(path, text):
    target = OUT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8", newline="\n")


def png(path, pixel, size=64):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    raw = b"".join(b"\0" + bytes(c for x in range(size) for c in pixel(x, y, size)) for y in range(size))
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    target = OUT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)


def mesh(name, vertices, faces):
    lines = [f"# Whatever Starter / {name}; original procedural geometry"]
    # Per-corner normals keep flat surfaces crisp. UVs are explicit and reusable.
    for face in faces:
        for i in range(1, len(face) - 1):
            tri = [vertices[face[k]] for k in (0, i, i + 1)]
            a = [tri[1][k] - tri[0][k] for k in range(3)]
            b = [tri[2][k] - tri[0][k] for k in range(3)]
            n = [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
            length = math.sqrt(sum(v*v for v in n))
            if length < 1e-10:
                continue
            normal = [v / length for v in n]
            base = sum(line.startswith("v ") for line in lines) + 1
            for point, uv in zip(tri, ((0, 0), (1, 0), (1, 1))):
                lines.append("v " + " ".join(f"{v:.6f}" for v in point))
                lines.append(f"vt {uv[0]} {uv[1]}")
                lines.append("vn " + " ".join(f"{v:.6f}" for v in normal))
            lines.append("f " + " ".join(f"{base+k}/{base+k}/{base+k}" for k in range(3)))
    write(f"meshes/{name}.obj", "\n".join(lines) + "\n")


def lathe(name, profile, segments=24, upright=False):
    vertices = [(radius*math.cos(i*math.tau/segments), y, radius*math.sin(i*math.tau/segments)) for y, radius in profile for i in range(segments)]
    if upright:
        vertices = [(x, -z, y) for x, y, z in vertices]
    faces = []
    for j in range(len(profile)-1):
        for i in range(segments):
            n = (i+1) % segments
            faces.append((j*segments+i, (j+1)*segments+i, (j+1)*segments+n, j*segments+n))
    mesh(name, vertices, faces)


def transform(position=(0, 0, 0), scale=(1, 1, 1), yaw=0):
    a = math.radians(yaw) / 2
    return {"position": list(position), "rotation": [0, math.sin(a), 0, math.cos(a)], "scale": list(scale)}


def part(name, shape="cube", texture="white", position=(0, 0, 0), scale=(1, 1, 1), yaw=0, shader="mesh_lit"):
    return {"name": name, "components": {"core:transform": transform(position, scale, yaw), "core:mesh_renderer": {
        "mesh": f"whatever_starter://meshes/{shape}.obj", "shader": f"whatever_starter://shaders/{shader}.wgsl", "texture": f"whatever_starter://textures/{texture}.png"}}}


def prefab(name, category, entity, description=""):
    entity.setdefault("name", name)
    entity.setdefault("script_id", name.lower().replace(" ", "_"))
    entity.setdefault("components", {}).setdefault("core:transform", transform())
    path = f"prefabs/{category.lower().replace(' ', '_')}/{name.lower().replace(' ', '_')}.wprefab"
    write(path, json.dumps({"version": 1, "name": name, "description": description or f"Editable {name.lower()} hierarchy from Whatever Starter.", "category": category, "entity": entity}, indent=2) + "\n")


write("wasset.toml", '''format_version = 1
id = "whatever_starter"
name = "Whatever Starter"
version = "1.0.0"
authors = ["Whatever Engine contributors"]
license = "Apache-2.0 AND OFL-1.1"
description = "Editable essentials: geometry, modular props, surfaces, sprites, sounds, shaders and scripted prefabs."
''')
write("LICENSE", (ROOT / "LICENSE").read_text(encoding="utf-8"))

# Unit geometry and reusable profiles; the original cube/sphere/capsule remain available.
lathe("cylinder", [(-.5,0),(-.5,.5),(.5,.5),(.5,0)])
lathe("cone", [(-.5,0),(-.5,.5),(.5,0)])
lathe("pyramid", [(-.5,0),(-.5,.7),(.5,0)], 4)
lathe("hex_column", [(-.5,0),(-.5,.5),(.5,.5),(.5,0)], 6)
lathe("oct_column", [(-.5,0),(-.5,.5),(.5,.5),(.5,0)], 8)
lathe("disc", [(-.04,0),(-.04,.5),(.04,.5),(.04,0)])
lathe("coin", [(-.06,0),(-.06,.46),(-.04,.5),(.04,.5),(.06,.46),(.06,0)], 32, upright=True)
lathe("barrel", [(-.5,0),(-.5,.35),(-.36,.4),(0,.46),(.36,.4),(.5,.35),(.5,0)], 16)
lathe("pot", [(-.4,0),(-.4,.24),(.3,.38),(.4,.4),(.4,.3),(.26,.28),(-.3,.15),(-.3,0)], 16)
lathe("bowl", [(-.2,0),(-.2,.16),(0,.4),(.2,.5),(.2,.44),(.05,.32),(-.12,.1),(-.12,0)])
lathe("vase", [(-.5,0),(-.5,.2),(-.3,.4),(.1,.3),(.35,.16),(.5,.2),(.5,.12),(.3,.09),(-.35,.1),(-.35,0)], 20)
lathe("gem", [(-.6,0),(.1,.45),(.4,.3),(.4,0)], 6)
lathe("diamond", [(-.6,0),(0,.45),(.6,0)], 8)
lathe("tree_canopy", [(-.7,0),(-.7,.75),(.1,.4),(.7,0)], 12)
lathe("low_sphere", [(math.sin(-math.pi/2 + j*math.pi/6)*.5, math.cos(-math.pi/2 + j*math.pi/6)*.5) for j in range(7)], 10)
lathe("capsule_low", [(-.9,0),(-.85,.22),(-.65,.4),(-.5,.45),(.5,.45),(.65,.4),(.85,.22),(.9,0)], 12)
lathe("pipe", [(-.5,.38),(-.5,.5),(.5,.5),(.5,.38),(-.5,.38)])
lathe("pedestal", [(-.5,0),(-.5,.5),(-.36,.5),(-.3,.3),(.3,.3),(.36,.5),(.5,.5),(.5,0)], 16)
mesh("plane", [(-.5,0,-.5),(-.5,0,.5),(.5,0,.5),(.5,0,-.5)], [(0,1,2,3)])
mesh("quad", [(-.5,-.5,0),(.5,-.5,0),(.5,.5,0),(-.5,.5,0)], [(0,1,2,3)])
mesh("ramp", [(-.5,-.5,-.5),(.5,-.5,-.5),(.5,-.5,.5),(-.5,-.5,.5),(-.5,.5,.5),(.5,.5,.5)], [(1,2,3,0),(2,5,4,3),(3,4,0),(5,2,1),(4,5,1,0)])
mesh("roof", [(-.5,-.5,-.5),(.5,-.5,-.5),(.5,-.5,.5),(-.5,-.5,.5),(0,.5,-.5),(0,.5,.5)], [(1,2,3,0),(3,5,4,0),(4,5,2,1),(4,1,0),(2,5,3)])
for name, major, minor, rings, segments in [("torus",.38,.12,24,12),("ring",.44,.06,32,8)]:
    vertices = [((major+minor*math.cos(j*math.tau/segments))*math.cos(i*math.tau/rings), minor*math.sin(j*math.tau/segments), (major+minor*math.cos(j*math.tau/segments))*math.sin(i*math.tau/rings)) for i in range(rings) for j in range(segments)]
    faces = [(i*segments+j, i*segments+(j+1)%segments, ((i+1)%rings)*segments+(j+1)%segments, ((i+1)%rings)*segments+j) for i in range(rings) for j in range(segments)]
    mesh(name, vertices, faces)
for index in range(6):
    rng = random.Random(100+index)
    vertices = []
    for y, radius in [(-.45,.3),(-.2,.55),(.2,.5),(.5,.22)]:
        for i in range(7):
            r = radius*rng.uniform(.8,1.2)
            vertices.append((r*math.cos(i*math.tau/7),y+rng.uniform(-.08,.08),r*math.sin(i*math.tau/7)))
    faces = [tuple(range(7)),tuple(reversed(range(21,28)))]
    faces += [(j*7+i,(j+1)*7+i,(j+1)*7+(i+1)%7,j*7+(i+1)%7) for j in range(3) for i in range(7)]
    mesh(f"rock_{index+1}", vertices, faces)

palette = {"white":(240,242,245),"black":(24,29,36),"slate":(78,97,117),"silver":(174,190,203),"red":(220,65,61),"orange":(242,142,54),"gold":(245,197,64),"yellow":(245,225,87),"lime":(161,202,72),"green":(58,149,95),"forest":(38,92,64),"mint":(100,208,170),"cyan":(66,187,209),"blue":(69,124,216),"navy":(42,60,106),"purple":(137,97,190),"pink":(227,133,176),"cream":(233,218,186),"brown":(128,85,57),"dark_wood":(75,48,38),"terracotta":(177,86,62),"sand":(203,179,127),"stone":(134,139,145),"snow":(224,236,242)}
for name, rgb in palette.items():
    png(f"textures/{name}.png", lambda x,y,s,c=rgb: (*c,255), 8)
for material, base in [("brick",(165,77,58)),("cobble",(111,123,133)),("grass",(78,123,62)),("sandstone",(192,166,120)),("concrete",(153,157,161)),("wood",(139,98,61)),("metal",(108,129,146)),("water",(54,128,168)),("roof_tiles",(134,65,53)),("marble",(205,211,218)),("dirt",(114,87,62)),("snow",(222,234,241))]:
    def surface(x,y,s,b=base,m=material):
        grain = math.sin(x*1.7+y*2.3)*4 + math.sin(x*.39-y*.17)*5
        if m in ["brick","roof_tiles"]:
            grain += -28 if y%16<2 or (x+(16 if (y//16)%2 else 0))%32<2 else 4
        elif m == "wood": grain += 11*math.sin(x*.9+3*math.sin(y*.1))
        elif m == "cobble": grain += -23 if (x+(8 if (y//16)%2 else 0))%16<2 or y%16<2 else 6
        elif m == "water": grain += 13*math.sin(x*math.tau/s*3+y*math.tau/s*2)
        elif m == "metal": grain += 10 if x%16<2 and y%16<2 else 0
        elif m == "marble": grain += 24*math.sin(x*.2+y*.16+math.sin(x*.1))**8
        return tuple(max(0,min(255,round(c+grain))) for c in b)+(255,)
    png(f"textures/{material}_tile.png",surface)
for a,b in [("white","black"),("blue","white"),("gold","black"),("red","white"),("green","white")]:
    png(f"textures/checker_{a}_{b}.png",lambda x,y,s,a=a,b=b: (*(palette[a] if (x//8+y//8)%2 else palette[b]),255))
    png(f"textures/stripe_{a}_{b}.png",lambda x,y,s,a=a,b=b: (*(palette[a] if ((x+y)//8)%2 else palette[b]),255))
png("textures/uv_grid.png",lambda x,y,s: (255 if x%8==0 else x*4,255 if y%8==0 else y*4,128,255))
png("textures/transparent.png",lambda x,y,s: (255,255,255,0),8)

# Transparent UI symbols, useful as sprites or custom HUD textures.
icons = ["circle","ring","square","diamond","triangle","plus","minus","cross","arrow_up","arrow_down","arrow_left","arrow_right","play","pause","stop","check","heart","shield","star","crosshair","reticle","bar","panel","button","key","coin","gem","marker","dot","grid"]
for name in icons:
    def icon(x,y,s,n=name):
        u,v=(x+.5)/s*2-1,(y+.5)/s*2-1
        r=math.hypot(u,v)
        show=False
        if n in ["circle","coin","dot"]: show=r<(.28 if n=="dot" else .78)
        elif n in ["ring","reticle"]: show=.62<r<.78
        elif n in ["square","stop","panel","button"]: show=abs(u)<.8 and abs(v)<(.38 if n=="button" else .75)
        elif n in ["diamond","gem"]: show=abs(u)+abs(v)<.9
        elif n in ["triangle","play"]:
            if n=="play": u,v=-v,u
            show=abs(u)<(.8-v)*.5 and -.8<v<.8
        elif n in ["plus","crosshair"]: show=(abs(u)<.1 and abs(v)<.75) or (abs(v)<.1 and abs(u)<.75)
        elif n=="minus": show=abs(v)<.1 and abs(u)<.75
        elif n=="cross": show=min(abs(u-v),abs(u+v))<.15 and max(abs(u),abs(v))<.75
        elif n.startswith("arrow"):
            if n.endswith("down"): v=-v
            if n.endswith("left"): u,v=v,u
            if n.endswith("right"): u,v=v,-u
            show=(abs(u)<.15 and -.4<v<.8) or (abs(u)<(.85+v) and -.85<v<-.3)
        elif n=="pause": show=.15<abs(u)<.5 and abs(v)<.7
        elif n=="check": show=(abs(v-u-.3)<.14 and -.65<u<-.1) or (abs(v+u+.1)<.14 and -.1<u<.75)
        elif n=="heart": show=(u*u+v*v-0.45)**3-u*u*(-v)**3<0
        elif n=="shield": show=abs(u)<.7 and -.8<v<.85-abs(u)*.8
        elif n=="star": show=r<.48+.25*math.cos(5*(math.atan2(v,u)+math.pi/2))
        elif n=="bar": show=abs(v)<.16 and abs(u)<.85
        elif n=="key": show=(.38<math.hypot(u+.35,v)<.55) or (abs(v)<.1 and -.1<u<.8) or (.5<u<.65 and 0<v<.35)
        elif n=="marker": show=math.hypot(u,v+.2)<.55 or (abs(u)<(.9-v)*.5 and 0<v<.9)
        elif n=="grid": show=(abs(u)<.8 and abs(v)<.8) and (abs(u)<.05 or abs(v)<.05 or abs(abs(u)-.7)<.05 or abs(abs(v)-.7)<.05)
        return (238,244,255,255 if show else 0)
    png(f"sprites/ui/{name}.png",icon)

# Contract-compatible material shaders. All lighting stays asset-owned.
lit=(OUT/"shaders/mesh_lit.wgsl").read_text()
for name, expression in {
    "mesh_unlit":"return base;",
    "mesh_normals":"return vec4<f32>(N * 0.5 + vec3<f32>(0.5), base.a);",
    "mesh_toon":"let shade = floor(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * 4.0) / 4.0; return vec4<f32>(shade, base.a);",
    "mesh_rim":"let rim = pow(1.0 - max(dot(N, V), 0.0), 3.0); return vec4<f32>(color + rim * vec3<f32>(0.25,0.6,0.9), base.a);",
    "mesh_grayscale":"let g = dot(color, vec3<f32>(0.2126,0.7152,0.0722)); return vec4<f32>(g,g,g,base.a);",
    "mesh_uv":"return vec4<f32>(fract(in.tex_coords),0.0,1.0);",
}.items():
    write(f"shaders/{name}.wgsl",lit.replace("return vec4<f32>(color, base.a);",expression))

shapes=["cube","sphere","bean","cylinder","cone","pyramid","hex_column","oct_column","disc","coin","barrel","pot","bowl","vase","gem","diamond","tree_canopy","low_sphere","capsule_low","pipe","pedestal","plane","quad","ramp","roof","torus","ring"]
for shape in shapes:
    prefab(shape.replace('_',' ').title(),"Primitives",part("",shape))
for index in range(6): prefab(f"Rock {index+1}","Nature",part("",f"rock_{index+1}","stone"))
for name,leaves in [("Pine Tree","forest"),("Autumn Pine","orange"),("Snow Pine","snow")]:
    prefab(name,"Nature",{"children":[part("Trunk","cylinder","wood_tile",(0,1,0),(.28,2,.28)),part("Lower canopy","tree_canopy",leaves,(0,1.5,0),(1.6,1.6,1.6)),part("Upper canopy","tree_canopy",leaves,(0,2.4,0),(1.2,1.5,1.2))]})
for name,color in [("Oak Tree","green"),("Autumn Tree","gold"),("Blossom Tree","pink")]:
    prefab(name,"Nature",{"children":[part("Trunk","cylinder","wood_tile",(0,1,0),(.35,2,.35)),part("Crown","low_sphere",color,(0,2.3,0),(3,2.5,3))]})
prefab("Bush","Nature",{"children":[part("Left","low_sphere","forest",(-.35,.4,0),(1.2,1,1)),part("Right","low_sphere","green",(.35,.5,0),(1.2,1.2,1))]})
prefab("Potted Plant","Nature",{"children":[part("Pot","pot","terracotta",(0,.4,0)),part("Foliage","low_sphere","green",(0,1,0),(1.2,1.5,1.2))]})
prefab("Mushroom","Nature",{"children":[part("Stem","cylinder","cream",(0,.4,0),(.3,.8,.3)),part("Cap","bowl","red",(0,.8,0),(1.4,-1,1.4))]})
for name,shape,tex in [("Crate","cube","wood_tile"),("Barrel","barrel","wood_tile"),("Stone Plinth","pedestal","marble_tile"),("Flower Pot","pot","terracotta"),("Storage Pipe","pipe","metal_tile"),("Gold Coin","coin","gold"),("Blue Gem","gem","cyan"),("Ruby","diamond","red")]:
    prefab(name,"Props",part("",shape,tex))
prefab("Table","Furniture",{"children":[part("Top","cube","wood_tile",(0,1,0),(2,.15,1.2))]+[part(f"Leg {i}","cube","dark_wood",(x,.5,z),(.15,1,.15)) for i,(x,z) in enumerate([(-.8,-.4),(.8,-.4),(-.8,.4),(.8,.4)])]})
prefab("Chair","Furniture",{"children":[part("Seat","cube","wood_tile",(0,.55,0),(.8,.12,.8)),part("Back","cube","wood_tile",(0,1,-.35),(.8,.9,.12))]+[part(f"Leg {i}","cube","dark_wood",(x,.25,z),(.1,.5,.1)) for i,(x,z) in enumerate([(-.3,-.3),(.3,-.3),(-.3,.3),(.3,.3)])]})
prefab("Bench","Furniture",{"children":[part("Seat","cube","wood_tile",(0,.6,0),(2,.15,.7)),part("Left support","cube","stone",(-.7,.3,0),(.2,.6,.6)),part("Right support","cube","stone",(.7,.3,0),(.2,.6,.6))]})
prefab("Bookshelf","Furniture",{"children":[part("Back","cube","dark_wood",(0,1,-.22),(1.5,2,.12))]+[part(f"Shelf {i}","cube","wood_tile",(0,i*.5,0),(1.5,.1,.6)) for i in range(5)]})
for name,tex in [("Wood Fence","wood_tile"),("Metal Railing","metal_tile")]:
    prefab(name,"Modular",{"children":[part("Post L","cube",tex,(-1,.6,0),(.15,1.2,.15)),part("Post R","cube",tex,(1,.6,0),(.15,1.2,.15)),part("Upper rail","cube",tex,(0,.9,0),(2,.12,.12)),part("Lower rail","cube",tex,(0,.4,0),(2,.12,.12))]})
for name,tex,scale in [("Wall","brick_tile",(4,3,.25)),("Floor","concrete_tile",(4,.2,4)),("Platform","metal_tile",(3,.3,3)),("Pillar","marble_tile",(.6,3,.6)),("Door","wood_tile",(1,2,.12)),("Window","cyan",(1,1,.08))]:
    prefab(name,"Modular",part("","cube",tex,(0,scale[1]/2,0),scale))
prefab("Stairs","Modular",{"children":[part(f"Step {i+1}","cube","stone",(0,(i+1)*.15,i*.4),(2,(i+1)*.3,.4)) for i in range(8)]})
prefab("Ramp","Modular",part("","ramp","concrete_tile",(0,1,0),(2,2,4)))
prefab("Bridge","Modular",{"children":[part("Deck","cube","wood_tile",(0,1,0),(3,.2,6)),part("Rail L","cube","dark_wood",(-1.4,1.7,0),(.12,.12,6)),part("Rail R","cube","dark_wood",(1.4,1.7,0),(.12,.12,6))]})
prefab("Archway","Modular",{"children":[part("Pillar L","cube","stone",(-1.3,1.5,0),(.6,3,.6)),part("Pillar R","cube","stone",(1.3,1.5,0),(.6,3,.6)),part("Lintel","cube","stone",(0,3,0),(3.2,.6,.6))]})
prefab("House","Buildings",{"children":[part("Walls","cube","cream",(0,1.5,0),(4,3,3)),part("Roof","roof","roof_tiles_tile",(0,3.7,0),(4.5,1.6,3.5)),part("Door","cube","wood_tile",(0,1,1.52),(.9,2,.1)),part("Window","cube","cyan",(1.2,1.8,1.53),(.8,.8,.1))]})
prefab("Tower","Buildings",{"children":[part("Tower","hex_column","stone",(0,3,0),(3,6,3)),part("Roof","cone","navy",(0,6.8,0),(4,2,4))]})
prefab("Well","Buildings",{"children":[part("Stone rim","pipe","stone",(0,.6,0),(2,1.2,2)),part("Post L","cube","wood_tile",(-1,1.6,0),(.15,3.2,.15)),part("Post R","cube","wood_tile",(1,1.6,0),(.15,3.2,.15)),part("Roof","roof","roof_tiles_tile",(0,3.3,0),(2.8,1,2))]})
for name,color in [("Warm Lamp","gold"),("Cool Lamp","cyan"),("Red Beacon","red")]:
    light=part("Bulb","sphere",color,(0,2.2,0),(.35,.35,.35),shader="mesh_unlit")
    light["components"]["core:point_light"]={"color":[c/255 for c in palette[color]],"intensity":2,"range":8}
    prefab(name,"Lighting",{"children":[part("Post","cylinder","black",(0,1,0),(.12,2,.12)),light]})
prefab("Daylight Rig","Lighting",{"children":[{"name":"Sky","components":{"core:ambient_light":{"color":[.7,.85,1],"intensity":.3}}},{"name":"Sun","components":{"core:directional_light":{"color":[1,.94,.82],"direction":[.4,1,.6],"intensity":1.1}}}]})
prefab("Nightlight Rig","Lighting",{"children":[{"name":"Night sky","components":{"core:ambient_light":{"color":[.2,.3,.6],"intensity":.2}}},{"name":"Moon","components":{"core:directional_light":{"color":[.5,.65,1],"direction":[-.3,1,-.5],"intensity":.45}}}]})
for name in icons:
    prefab(name.replace('_',' ').title()+" Sprite","Sprites",{"components":{"core:sprite_renderer":{"texture":f"whatever_starter://sprites/ui/{name}.png","shader":"whatever_starter://shaders/sprite.wgsl"}}})
for material in ["brick","cobble","grass","sandstone","concrete","wood","metal","water","roof_tiles","marble","dirt","snow"]:
    prefab(material.replace('_',' ').title()+" Surface","Surfaces",part("","cube",material+"_tile",scale=(3,.15,3)))
for shader in ["lit","unlit","normals","toon","rim","grayscale","uv"]:
    prefab(shader.title()+" Material Sample","Materials",part("","sphere","checker_blue_white",shader="mesh_"+shader))

# Reusable mod code. Private state is intentionally not persisted into prefab schemas.
header='import { Component, Time, Type, field } from "@whatever-engine/api";\n\n'
scripts={
"StarterSpin": '''  speed = field(Type.F64, 60);
  private angle = 0;
  override tick(): void {
    this.angle += this.speed * Time.deltaSeconds;
    this.entity.setEulerDegrees(0, this.angle, 0);
  }
''',
"StarterBob": '''  amplitude = field(Type.F64, 0.3);
  frequency = field(Type.F64, 1);
  private elapsed = 0;
  private origin = 0;
  override start(): void {
    this.origin = this.entity.getComponent("core:transform")?.position[1] ?? 0;
  }
  override tick(): void {
    this.elapsed += Time.deltaSeconds;
    this.entity.editComponent("core:transform", (pose) => {
      pose.position[1] = this.origin + Math.sin(this.elapsed * this.frequency * Math.PI * 2) * this.amplitude;
    });
  }
''',
"StarterPulse": '''  amplitude = field(Type.F64, 0.15);
  frequency = field(Type.F64, 1);
  private elapsed = 0;
  private base: [number, number, number] = [1, 1, 1];
  override start(): void {
    const scale = this.entity.getComponent("core:transform")?.scale;
    if (scale) this.base = [...scale];
  }
  override tick(): void {
    this.elapsed += Time.deltaSeconds;
    const factor = 1 + Math.sin(this.elapsed * this.frequency * Math.PI * 2) * this.amplitude;
    this.entity.editComponent("core:transform", (pose) => {
      pose.scale = [this.base[0] * factor, this.base[1] * factor, this.base[2] * factor];
    });
  }
''',
"StarterLifetime": '''  seconds = field(Type.F64, 3);
  private elapsed = 0;
  override tick(): void {
    this.elapsed += Time.deltaSeconds;
    if (this.elapsed >= this.seconds) this.entity.destroy();
  }
''',
"StarterOrbit": '''  radius = field(Type.F64, 2);
  speed = field(Type.F64, 30);
  private angle = 0;
  private x = 0;
  private z = 0;
  override start(): void {
    const pose = this.entity.getComponent("core:transform");
    this.x = pose?.position[0] ?? 0;
    this.z = pose?.position[2] ?? 0;
  }
  override tick(): void {
    this.angle += this.speed * Time.deltaSeconds * Math.PI / 180;
    this.entity.editComponent("core:transform", (pose) => {
      pose.position[0] = this.x + Math.cos(this.angle) * this.radius;
      pose.position[2] = this.z + Math.sin(this.angle) * this.radius;
    });
  }
''',
"StarterTurntable": '''  speed = field(Type.F64, 20);
  tilt = field(Type.F64, 12);
  private angle = 0;
  override tick(): void {
    this.angle += this.speed * Time.deltaSeconds;
    this.entity.setEulerDegrees(this.tilt, this.angle, 0);
  }
'''}
for name,body in scripts.items():
    write(f"scripts/{name}.ts",header+f'export class {name} extends Component {{\n  static readonly type = "whatever_starter:{name}";\n'+body+'}\n')
for name,shape,color,behaviors in [("Spinning Coin","coin","gold",["StarterSpin","StarterBob"]),("Floating Gem","gem","cyan",["StarterSpin","StarterBob"]),("Pulsing Heart","low_sphere","red",["StarterPulse"]),("Orbiting Orb","sphere","purple",["StarterOrbit"]),("Turntable Display","pedestal","stone",["StarterTurntable"]),("Temporary Marker","cone","orange",["StarterLifetime"])]:
    entity=part("",shape,color)
    entity["components"].update({"whatever_starter:"+behavior:{} for behavior in behaviors})
    prefab(name,"Scripted",entity,"Editable visual behavior. Requires a script-enabled receiving mod; no gameplay logic is installed into the engine.")

# A complete self-contained physics hierarchy illustrates local entity references.
physics={"name":"Physics Sandbox","script_id":"physics","components":{"core:physics_world":{"gravity":[0,-9.81,0]}},"children":[]}
floor=part("Floor","cube","checker_white_black",(0,-.2,0),(8,.4,8))
floor["components"]["core:collider"]={"world":{"$entity":"physics"},"body":None,"shape":{"type":"box","size":[8,.4,8]}}
physics["children"].append(floor)
for i in range(4):
    body=part(f"Box {i+1}","cube",["red","gold","green","blue"][i],(0,1+i*1.1,0))
    body["script_id"]=f"body_{i}"
    body["components"]["core:rigid_body"]={"world":{"$entity":"physics"},"type":"dynamic"}
    body["components"]["core:collider"]={"body":{"$entity":f"body_{i}"},"shape":{"type":"box","size":[1,1,1]}}
    physics["children"].append(body)
prefab("Physics Sandbox","Physics",physics)
prefab("Camera Rig","Cameras",{"children":[{"name":"Camera","script_id":"camera","components":{"core:transform":transform((0,2,8)),"core:camera":{"fovy_degrees":60,"znear":.1,"zfar":1000}}}]})

for name,freq,duration,mode in [("ui_click",900,.06,"tone"),("ui_confirm",660,.18,"rise"),("ui_cancel",440,.18,"fall"),("pickup",880,.24,"rise"),("coin",1320,.18,"rise"),("jump",220,.22,"rise"),("land",120,.15,"noise"),("hit",180,.18,"noise"),("laser",1200,.25,"fall"),("power_up",330,.5,"rise"),("error",170,.3,"tone"),("notification",740,.3,"tone"),("step",100,.08,"noise"),("switch",400,.08,"tone"),("explosion",80,.55,"noise"),("success",523,.6,"rise")]:
    path=OUT/f"audio/{name}.wav"
    path.parent.mkdir(parents=True,exist_ok=True)
    rng=random.Random(name)
    samples=[]
    phase=0
    for i in range(int(22050*duration)):
        t=i/22050
        progress=t/duration
        pitch=freq*(1+progress*2 if mode=="rise" else 1-progress*.8 if mode=="fall" else 1)
        phase+=pitch*math.tau/22050
        sample=rng.uniform(-1,1) if mode=="noise" else math.sin(phase)
        envelope=min(1,t/.008)*(1-progress)**2
        samples.append(struct.pack("<h",round(sample*envelope*13000)))
    with wave.open(str(path),"wb") as f:
        f.setnchannels(1); f.setsampwidth(2); f.setframerate(22050); f.writeframes(b"".join(samples))

counts={folder:len([p for p in (OUT/folder).rglob('*') if p.is_file() and (folder != "fonts" or p.suffix in [".ttf", ".otf"])]) for folder in ["meshes","textures","sprites","shaders","prefabs","scripts","audio","fonts"]}
write("catalog.json",json.dumps({"version":1,"counts":counts},indent=2)+"\n")
write("README.md",f'''# Whatever Starter

An editable starter library maintained with Whatever Engine. Import the .wasset
through Project > Import Asset Library, or choose an Official prefab from Scene >
Instantiate Prefab. Contents are copied into the receiving mod's libraries folder.
Reimporting preserves edited bundles by creating another copy; it never upgrades
existing scene objects. Prefab instances are independent hierarchies.

Contents: {', '.join(f'{count} {kind}' for kind,count in counts.items())}.

Primitives use unit dimensions except the original sphere/capsule. Modular pieces
are intended for meter-like units. Meshes use explicit normals and UVs. Materials
are supplied through shaders and textures because OBJ materials are not loaded.
Sprites have transparent backgrounds. Audio is original 22.05 kHz mono PCM WAV.
Physics Sandbox owns its world; its local references are remapped per instance.
Scripted prefabs need a script-enabled mod. The six exported components are loaded
by the normal sandboxed component host and expose editable fields in Inspector.

All content except Noto Sans is covered by LICENSE (Apache-2.0). Noto Sans is
Copyright 2022 The Noto Project Authors and is distributed under fonts/OFL.txt.
Run python tools/generate-starter.py from the source checkout to regenerate the
original content. Existing Noto Sans and its license are retained unchanged.

Library authoring paths use whatever_starter:// and whatever_starter:ComponentName.
Import rewrites those prefixes to the receiving mod and imported folder. Relative
JavaScript/TypeScript imports remain relative. Scripts do not run during import.
''')
print(json.dumps(counts))
