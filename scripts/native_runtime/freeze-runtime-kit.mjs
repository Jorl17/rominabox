// For developers: freeze a finished submodule build into the prepared kit.
// Old build directories stay as they are.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
const root=path.resolve(import.meta.dirname,'../..');
if(process.argv.length!==3){console.error('Usage: node scripts/native_runtime/freeze-runtime-kit.mjs BUILD_DIRECTORY');process.exit(2);}
const build=path.resolve(process.argv[2]),kit=path.join(root,'desktop/src-tauri/resources/runtime');
const info=JSON.parse(fs.readFileSync(path.join(build,'build-info.json'),'utf8'));
if(info.testOnly)throw Error('A test-only achievements build must never be frozen into the distributable kit');
if(info.capabilities?.achievements!==true)throw Error('Build info has no verified achievements capability');
const source=path.join(root,'vendor/retroarch');
const revision=execFileSync('git',['rev-parse',info.retroarchCommit+'^{commit}'],{cwd:source,encoding:'utf8'}).trim();
const frozen=path.join(fs.mkdtempSync(path.join(build,'frozen-')),'retroarch');
// We get the CLI from scripts/built.py, with which we build it from this
// checkout and reject a CLI built in another checkout.
const cli=execFileSync('python3',[path.join(root,'scripts/built.py'),'--build'],{cwd:root,encoding:'utf8'}).trim().split('\n').pop();
execFileSync(cli,['freeze-macos-executable'],{input:JSON.stringify({source:path.join(build,'retroarch/retroarch'),destination:frozen}),stdio:['pipe','inherit','inherit']});
const binary=path.join(kit,'bin/retroarch');
const dependencies=execFileSync('otool',['-L',frozen],{encoding:'utf8'}).split('\n').slice(1).map(x=>x.trim().split(' (')[0]).filter(x=>x.startsWith('@executable_path/Frameworks/'));
for(const dep of dependencies){if(!fs.existsSync(path.join(kit,'Frameworks',path.basename(dep))))throw Error('Runtime kit missing '+dep);}
fs.copyFileSync(frozen,binary);
for(const dep of dependencies){
  const name=path.basename(dep);
  if(!fs.existsSync(path.join(kit,'Frameworks',name)))throw Error('Runtime kit missing dependency '+name);
  execFileSync('install_name_tool',['-change',dep,'@executable_path/../Frameworks/'+name,binary]);
}
execFileSync('codesign',['--force','--sign','-',binary]);
const sha=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const mf=path.join(kit,'manifest.json'),manifest=JSON.parse(fs.readFileSync(mf,'utf8'));
const ra=manifest.components.find(c=>c.name==='RetroArch');
Object.assign(ra,{capabilities:info.capabilities,revision,origin:'Built from the pinned ROM-in-a-Box RetroArch submodule; private development build.',source_url:'https://github.com/Jorl17/rominabox-retroarch/tree/'+revision,binary_sha256:sha(binary),integration_provenance:'provenance/native-rmlui/source.json'});
delete ra.integration_patch_sha256;delete ra.source_archive;delete ra.source_sha256;
fs.writeFileSync(mf,JSON.stringify(manifest,null,2)+'\n');
const provenance=path.join(kit,'provenance/native-rmlui');
fs.writeFileSync(path.join(provenance,'source.json'),JSON.stringify({...info,retroarchRepository:'https://github.com/Jorl17/rominabox-retroarch'},null,2)+'\n');
fs.writeFileSync(path.join(provenance,'README.txt'),'Current source is the pinned private RetroArch downstream commit recorded in source.json and the runtime manifest. Build with scripts/native_runtime/build-retroarch-rmlui-macos.sh, then freeze-runtime-kit.mjs. The file retroarch-rmlui.patch is not applied by current builds. Upstream licenses remain unchanged. Public redistribution and complete corresponding-source review remain separate.\n');
console.log('Frozen downstream commit '+revision);
