"""Rebuild Coinwatch's editable scene and two game-specific prefabs from imported starter assets."""
from pathlib import Path
import copy, json, math
ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / 'games/coinwatch/mods/core'
LIB = CORE / 'libraries/whatever_starter'
URI = 'core://libraries/whatever_starter/'
used = set()
def transform(p=(0,0,0), s=(1,1,1), yaw=0):
    return dict(position=list(p), scale=list(s), rotation=[0,math.sin(yaw/2),0,math.cos(yaw/2)])
def entity(name, id=None, p=(0,0,0), components=None, children=None, scale=(1,1,1)):
    e=dict(name=name, components={'core:transform':transform(p,scale), **(components or {})})
    if id: e['script_id']=id
    if children: e['children']=children
    return e
def mesh(name,p,size,texture='cream',shape='cube',shader='mesh_lit',id=None):
    return entity(name,id,p,{'core:mesh_renderer':dict(mesh=URI+'meshes/'+shape+'.obj',shader=URI+'shaders/'+shader+'.wgsl',texture=URI+'textures/'+texture+'.png')},scale=size)
def prefab(path, p=(0,0,0), yaw=0, scale=1):
    used.add(path)
    e=copy.deepcopy(json.loads((LIB/'prefabs'/f'{path}.wprefab').read_text())['entity'])
    def clean(node):
        node.pop('script_id',None)
        for child in node.get('children',[]): clean(child)
    clean(e)
    e['name']=path.split('/')[-1].replace('_',' ').title()
    old=e.get('components',{}).get('core:transform',transform())
    old['position']=list(p);old['rotation']=[0,math.sin(yaw/2),0,math.cos(yaw/2)];old['scale']=[v*scale for v in old.get('scale',[1,1,1])]
    e.setdefault('components',{})['core:transform']=old
    return e
def solid(e,id,size,offset=(0,0,0)):
    e['script_id']=id
    e['components']['core:rigid_body']=dict(type='fixed',world={'$entity':'world'})
    e['components']['core:collider']=dict(body={'$entity':id},shape=dict(type='box',size=list(size)),position=list(offset),friction=.7)
    return e
def text(name,id,p,value,size=48,color=(1,.91,.55,1),scale=(1,1,1)):
    e=entity(name,id,p,{'core:text_renderer':dict(text=value,font=URI+'fonts/default.ttf',shader=URI+'shaders/sprite.wgsl',font_size=size,color=list(color))},scale=scale)
    e['components']['core:transform']['rotation']=[math.sqrt(.5),0,0,math.sqrt(.5)]
    return e
def write(path,data):
    path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(data,indent=2,ensure_ascii=False)+'\n')
scene=[entity('Run Session — score and lifecycle','rules',components={'core:RunSession':{},'core:RunState':{},'core:GameWindow':{},'core:RestartInput':{'session':{'$component':'core:RunSession','$entity':'rules'}},'core:SoundEffects':{'session':{'$component':'core:RunSession','$entity':'rules'}},'core:PlaytestTools':{'session':{'$component':'core:RunSession','$entity':'rules'},'player':{'$entity':'player'},'spawner':{'$entity':'spawner'},'weapon':{'$entity':'camera'}}}),
 entity('Physics World','world',components={'core:physics_world':dict(substeps=2,rapier=dict(integration=dict(max_ccd_substeps=4,num_solver_iterations=8)))}),
 entity('Daylight',components={'core:ambient_light':dict(color=[.72,.84,1],intensity=.55),'core:directional_light':dict(direction=[.4,1,.6],color=[1,.92,.8],intensity=1.3)}),
 solid(mesh('Meadow foundation',(0,-.6,0),(52,1,52),'grass_tile'),'ground',(52,1,52)),
 mesh('Central cobblestone plaza',(0,-.07,0),(17,.12,17),'cobble_tile')]
for x in (-20,-8,0,8,20):
    scene.append(mesh('North south stone lane',(x,-.065,0),(2.8,.12,47),'concrete_tile'))
for z in (-20,-8,0,8,20):
    scene.append(mesh('East west stone lane',(0,-.06,z),(47,.12,2.8),'concrete_tile'))
for i,(p,size) in enumerate([((0,.8,-25),(51,1.8,1)),((0,.8,25),(51,1.8,1)),((-25,.8,0),(1,1.8,51)),((25,.8,0),(1,1.8,51))]):
    scene.append(solid(mesh('Arena stone boundary',p,size,'brick_tile'),'boundary_'+str(i),size))
for i,(x,z) in enumerate([(-14,-12),(14,-12),(-14,12),(14,12)]):
    scene.append(solid(prefab('buildings/house',(x,-.1,z)),'house_'+str(i),(4,3,3),(0,1.5,0)))
    scene.append(prefab('nature/potted_plant',(x-2.6,0,z+2)))
    scene.append(prefab('props/flower_pot',(x+2.6,0,z+2)))
scene.append(solid(prefab('buildings/tower',(-22,0,-22)),'tower',(2,5,2),(0,2.5,0)))
scene.append(solid(prefab('buildings/well',(5,0,-14)),'well',(2.4,1.2,2.4),(0,.6,0)))
for i,(x,z) in enumerate([(-5,-5),(5,-5),(-5,5),(5,5)]):
    scene.append(solid(prefab('props/crate',(x,.5,z)),'crate_'+str(i),(1,1,1)))
    scene.append(solid(prefab('props/barrel',(x+1.3,.6,z)),'barrel_'+str(i),(.8,1.2,.8)))
    scene.append(prefab('nature/bush',(x,0,z+1.6),scale=.8))
for i,(x,z,kind) in enumerate([(-23,-15,'oak_tree'),(-23,0,'blossom_tree'),(-23,15,'autumn_tree'),(23,-15,'pine_tree'),(23,0,'autumn_pine'),(23,15,'snow_pine'),(-15,-23,'pine_tree'),(15,-23,'oak_tree'),(-15,23,'blossom_tree'),(15,23,'autumn_tree')]):
    scene.append(prefab('nature/'+kind,(x,0,z)))
    scene.append(prefab('nature/rock_'+str(i%6+1),(x+1.5,0,z-1.5),scale=.7))
    scene.append(prefab('nature/mushroom',(x-1.5,.1,z+1.5),scale=.8))
for x,z in [(-10,-4),(10,-4),(-10,4),(10,4)]:
    scene.append(prefab('furniture/bench',(x,0,z),yaw=math.pi/2))
    scene.append(prefab('lighting/warm_lamp',(x,0,z-2)))
scene.extend([prefab('furniture/table',(-17,0,-4)),prefab('furniture/chair',(-17,0,-5.5)),prefab('furniture/bookshelf',(17,0,-4)),prefab('props/storage_pipe',(17,.5,4)),prefab('props/stone_plinth',(-17,0,4)),prefab('props/blue_gem',(-17,1.3,4)),prefab('props/ruby',(17,1.4,-4))])
# Raised center board is world-space text at the arena origin, facing the starting camera.
scene.extend([mesh('Scoreboard panel',(0,3.15,-.25),(9.2,5.8,.3),'navy'),text('World origin score board','score',(0,3.9,0),'COINWATCH\nSCORE 0',40),text('Instructions','help',(0,.95,.02),'Collect gold. Shoot sentinels.\nRed: hunting 20s / Blue: roaming 10s\nESC releases mouse / R restarts',20,color=(.7,.9,1,1))])
reticle=text('Aim reticle','reticle',(0,0,-.5),'+',24,scale=(.06,.06,.06))
# Tiny view model is assembled from the starter meshes, with no collider.
weapon=entity('Pulse launcher',p=(.22,-.18,-.45),children=[mesh('Metal housing',(0,0,0),(.16,.14,.34),'metal_tile'),mesh('Energy core',(0,.03,-.2),(.065,.065,.12),'cyan','cylinder','mesh_unlit')])
scene.append(entity('Player — FPS movement / weapon settings','player',(0,1.05,9),{'core:FPSPlayer':{},'core:rigid_body':dict(type='dynamic',world={'$entity':'world'},lockedAxes=['rotationX','rotationY','rotationZ'],canSleep=False,ccd=True),'core:collider':dict(body={'$entity':'player'},shape=dict(type='capsule',radius=.4,height=1.8),mass=75,friction=0,events=['collision'])},children=[entity('Mouse yaw pivot','look',(0,.65,0),children=[entity('FPS Camera','camera',components={'core:camera':dict(fovy_degrees=75,znear=.03,zfar=180)},children=[reticle,weapon])])]))
death=entity('Game over camera — clear view of the score board','death_camera',(0,6,12),{'core:camera':{'fovy_degrees':55,'znear':.1,'zfar':180},'core:GameOverView':{'session':{'$component':'core:RunSession','$entity':'rules'},'playCamera':{'$entity':'camera'}}})
pitch=-math.atan2(2.5,12)
death['components']['core:transform']['rotation']=[math.sin(pitch/2),0,0,math.cos(pitch/2)]
scene.append(death)
# Explicit graph edges match the visible lanes. Nodes are editable in the hierarchy.
coords=[-20,-8,0,8,20]
nav=[]
for j,z in enumerate(coords):
    for i,x in enumerate(coords):
        key=f'n{i}_{j}'
        links=[f'n{a}_{b}' for a,b in [(i-1,j),(i+1,j),(i,j-1),(i,j+1)] if 0<=a<5 and 0<=b<5]
        nav.append(entity('Nav '+key,key,(x,1,z),{'core:NavNode':dict(key=key,links=links,spawnAllowed=(abs(x)>=20 or abs(z)>=20))}))
scene.append(entity('Navigation graph — 25 connected lane nodes','navigation',children=nav))
scene.append(entity('Enemy Spawner — cadence and population','spawner',components={'core:EnemySpawner':{'session':{'$component':'core:RunSession','$entity':'rules'},'world':{'$entity':'world'},'target':{'$entity':'player'},'graph':{'$entity':'navigation'}}}))
coin_positions=[(0,6),(0,3),(-3,8),(3,8),(-8,0),(8,0),(0,-8),(-8,-8),(8,-8),(-8,8),(8,8),(-20,0),(20,0),(0,-20),(0,20),(-20,-20),(20,-20),(-20,20),(20,20),(8,20),(-8,-20),(20,8),(-20,-8)]
for i,(x,z) in enumerate(coin_positions):
    e=prefab('scripted/spinning_coin',(x,1,z),scale=.55)
    e['script_id']=f'coin_{i}';e['name']=f'Gold Coin {i+1:02d}'
    e['components']['core:CoinPickup']={'session':{'$component':'core:RunSession','$entity':'rules'},'collector':{'$entity':'player'}}
    scene.append(e)
def compose(e):
    c=e['components']; name=e.get('script_id')
    if name=='player':
        c.pop('core:FPSPlayer',None)
        c['core:FPSMovement']={'session':{'$component':'core:RunSession','$entity':'rules'},'look':{'$entity':'look'}}
        c['core:PlayerLife']={'session':{'$component':'core:RunSession','$entity':'rules'}}
    if name=='look': c['core:MouseLook']={'session':{'$component':'core:RunSession','$entity':'rules'},'camera':{'$entity':'camera'}}
    if name=='camera': c['core:ProjectileWeapon']={'session':{'$component':'core:RunSession','$entity':'rules'},'world':{'$entity':'world'},'owner':{'$entity':'player'}}
    if name=='score': c['core:Scoreboard']={'session':{'$component':'core:RunSession','$entity':'rules'}}
    for child in e.get('children',[]): compose(child)
for e in scene: compose(e)
write(CORE/'scenes/main.wscene',dict(version=1,entities=scene))
# Distinct game prefab built from starter capsule, rings, spheres and unlit textures.
visual=mesh('Sentinel body',(0,0,0),(.8,1.1,.8),'orange','capsule_low')
visual['script_id']='visual'
visual['components']['core:EnemyAppearance']={'body':{'$entity':'enemy'}}
visual['children']=[mesh('Left eye',(-.18,.35,-.37),(.11,.11,.11),'cyan','low_sphere','mesh_unlit'),mesh('Right eye',(.18,.35,-.37),(.11,.11,.11),'cyan','low_sphere','mesh_unlit'),mesh('Halo',(0,.7,0),(.8,.12,.8),'gold','torus','mesh_unlit')]
write(CORE/'prefabs/sentinel.wprefab',dict(version=1,name='Coinwatch Sentinel',category='Coinwatch',description='Independent starter-mesh sentinel; Navigation, health, contact damage and appearance are independent configurable components; the spawner binds external entity references.',entity=entity('Sentinel','enemy',p=(0,-1000,0),components={'core:EnemyNavigation':{},'core:Health':{},'core:ContactDamage':{}},children=[visual])))
write(CORE/'prefabs/pulse.wprefab',dict(version=1,name='Pulse Projectile',category='Coinwatch',entity=mesh('Pulse',(0,-1000,0),(1,1,1),'cyan','low_sphere','mesh_unlit')))
write(ROOT/'games/coinwatch/starter-usage.json',dict(prefabs=sorted(used),placed_coin_count=len(coin_positions),nav_node_count=25))
print(f'Coinwatch authored with {len(used)} distinct starter prefabs, {len(coin_positions)} coins, 25 nav nodes.')

