#!/usr/bin/env python3
"""Run one real pinned-upstream snapshot pipeline and missing-artwork rejection."""
import argparse,hashlib,json,os,pathlib,shutil,subprocess,tempfile
PIN='9f38098d2355d4d77e41a538b48354fa8145d92a'
parser=argparse.ArgumentParser();parser.add_argument('--upstream',type=pathlib.Path,required=True);args=parser.parse_args()
fixture=pathlib.Path(__file__).resolve().parent;repo=fixture.parents[2]
work=pathlib.Path(tempfile.mkdtemp(prefix='tanglewood-p0-filter-'));snapshot=work/'snapshot';snapshot.mkdir()
for name in ('source.md','assets'):shutil.copytree(fixture/name,snapshot/name) if (fixture/name).is_dir() else shutil.copy2(fixture/name,snapshot/name)
runtime=work/'runtime/illustration_md';runtime.mkdir(parents=True)
for name in ('__init__.py','prepare.py','png.py'):
 data=subprocess.check_output(['git','-C',str(args.upstream),'show',f'{PIN}:illustration_md/{name}']);(runtime/name).write_bytes(data)
source=snapshot/'source.md';before=hashlib.sha256(source.read_bytes()).hexdigest();env=os.environ.copy();env.update(PYTHONPATH=str(runtime.parent),TANGLEWOOD_P0_SOURCE=str(source),TANGLEWOOD_P0_OUTPUT=str(work/'output'))
for writer in ('json','html'):
 command=['/usr/bin/pandoc','--from','markdown','--to',writer,'--lua-filter',str(repo/'docs/plan/examples/callout.lua'),'--filter',str(fixture/'prepare-stage.py'),str(source)]
 if writer=='html':command+=['--standalone','--embed-resources','--resource-path',str(snapshot),'--metadata','pagetitle=Saved artwork proof']
 output=subprocess.run(command,env=env,capture_output=True);assert output.returncode==0,output.stderr.decode();(work/f'output.{writer}').write_bytes(output.stdout)
 tree=json.loads((work/f'output.{writer}.prepared.json').read_text());figures=[node for node in tree['blocks'] if node['t']=='Figure'];assert len(figures)==2
 assert any(node['t']=='Div' and node['c'][0][0]=='callout-proof' and node['c'][1][0]['t']=='BlockQuote' for node in tree['blocks'])
 semantic=[{'id':node['c'][0][0],'caption':node['c'][1][1][0]['c'][0]['c'],'image':node['c'][2][0]['c'][0]['c']} for node in figures]
 if writer=='json':(work/'semantic.json').write_text(json.dumps(semantic,ensure_ascii=False,indent=2))
 else:
  assert semantic==json.loads((work/'semantic.json').read_text());assert b'data:image/png;base64,' in output.stdout and b'data:image/svg+xml' in output.stdout
assert hashlib.sha256(source.read_bytes()).hexdigest()==before
# Upstream rejects absent saved artwork, with original source retained.
asset=snapshot/'assets/fig-add-inputs/figure.svg';asset.rename(asset.with_suffix('.held'))
rejected=subprocess.run(['/usr/bin/pandoc','-f','markdown','-t','json','--lua-filter',str(repo/'docs/plan/examples/callout.lua'),'--filter',str(fixture/'prepare-stage.py'),str(source)],env=env,capture_output=True)
assert rejected.returncode!=0 and b'existing local SVG' in rejected.stderr
assert hashlib.sha256(source.read_bytes()).hexdigest()==before
(work/'rejected.stderr').write_bytes(rejected.stderr)
print(json.dumps({'result':'PASS','pandoc':subprocess.check_output(['/usr/bin/pandoc','--version']).decode().splitlines()[0],'upstream_commit':PIN,'source_sha256':before,'figures':2,'targets':['json','html'],'callout_before_prepare':True,'missing_svg_rejected':True,'source_unchanged':True,'evidence':str(work)},indent=2))
