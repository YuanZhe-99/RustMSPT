import re,sys
path,node=sys.argv[1],int(sys.argv[2])
t=open(path).read()
def arr(name,cast=float):
    m=re.search(r'<DataArray[^>]*Name="%s"[^>]*>([^<]*)<'%re.escape(name),t)
    return [cast(x) for x in m.group(1).split()] if m else None
pts=re.search(r'<Points>\s*<DataArray[^>]*>([^<]*)<',t).group(1).split()
P=[tuple(map(float,pts[i:i+3])) for i in range(0,len(pts),3)]
conn=arr('connectivity',int); off=arr('offsets',int); typ=arr('types',int)
rk=arr('region_key',int); prov=arr('provenance',int); par=arr('parent_cell',int); esc=arr('escalation_reason',int); plc=arr('plc_path',int)
rso=arr('RegionSetOffsets',int); rsc=arr('RegionSetComponents',int)
sets=[]; s=0
for o in rso: sets.append(rsc[s:o]); s=o
ck=arr('constraint_kind',int); no=arr('node_origin',int)
print('node',node,P[node],'constraint_kind',ck[node] if ck else None,'origin',no[node] if no else None)
start=0
for c,o in enumerate(off):
    cell=conn[start:o]; start=o
    if typ[c]!=10 or node not in cell: continue
    print(' tet',c,cell,'region',sets[rk[c]],'prov',prov[c],'parent',par[c] if par else None,'esc',esc[c] if esc else None,'plc',plc[c] if plc else None)
