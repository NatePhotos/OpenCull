# Requires onnxruntime-gpu[cuda,cudnn]==1.28.0, NumPy and Pillow. All photo data stays local.
import sys,os,json,time,resource,threading,subprocess,statistics,collections
from pathlib import Path
BASE=Path(os.environ.get('OPENCULL_BENCH_DIR', '/tmp/opencull-cuda-benchmark'))
PYTHON=sys.executable
MODELS=str(Path(__file__).resolve().parents[1]/'src-tauri'/'models')+'/'
CONFIGS={
 'CPU':None,
 'CUDA-default':{'use_tf32':'0'},
 'CUDA-arena-only':{'use_tf32':'0','arena_extend_strategy':'kSameAsRequested'},
 'CUDA-small-workspace':{'use_tf32':'0','cudnn_conv_use_max_workspace':'0'},
 'CUDA-heuristic':{'use_tf32':'0','cudnn_conv_use_max_workspace':'0','cudnn_conv_algo_search':'HEURISTIC'},
 'CUDA-tight-arena':{'use_tf32':'0','cudnn_conv_use_max_workspace':'0','cudnn_conv_algo_search':'HEURISTIC','arena_extend_strategy':'kSameAsRequested'},
 'CUDA-reuse-device-inputs':{'use_tf32':'0','cudnn_conv_use_max_workspace':'0','cudnn_conv_algo_search':'HEURISTIC','arena_extend_strategy':'kSameAsRequested'},
}
if len(sys.argv)>1 and sys.argv[1] in ['worker','verify']:
 import numpy as np
 import onnxruntime as ort
 mode,config,batch=sys.argv[1],sys.argv[2],int(sys.argv[3]);verify=mode=='verify'
 inputs=np.load(BASE/f'inputs-{batch}.npz')
 feeds={name:{key.split(':')[1]:np.ascontiguousarray(inputs[key]) for key in inputs.files if key.startswith(name+':')} for name in ['finder','judge','eyes','mouth']}
 def rss():
  return int(next(x.split()[1] for x in Path('/proc/self/status').read_text().splitlines() if x.startswith('VmRSS:')))/1024
 baseline=rss();start=time.perf_counter()
 if CONFIGS[config] is not None: ort.preload_dlls(directory='')
 sessions=[]
 for name,feed in feeds.items():
  opt=ort.SessionOptions();opt.intra_op_num_threads=1;opt.inter_op_num_threads=1;opt.graph_optimization_level=ort.GraphOptimizationLevel.ORT_ENABLE_ALL
  if verify:
   opt.enable_profiling=True;opt.profile_file_prefix=str(BASE/('tuning-profile-'+config+'-'+name+'-'+str(batch)))
  providers=['CPUExecutionProvider'] if config=='CPU' else [('CUDAExecutionProvider',CONFIGS[config]),'CPUExecutionProvider']
  s=ort.InferenceSession(MODELS+name+'.onnx',sess_options=opt,providers=providers);s.disable_fallback()
  if config!='CPU': assert 'CUDAExecutionProvider' in s.get_providers()
  binding=None;device={}
  if config=='CUDA-reuse-device-inputs':
   binding=s.io_binding()
   for key,value in feed.items():
    device[key]=ort.OrtValue.ortvalue_from_numpy(value,'cuda',0);binding.bind_ortvalue_input(key,device[key])
   for out in s.get_outputs():binding.bind_output(out.name,'cpu')
  sessions.append((name,s,feed,binding,device))
 setup_ms=(time.perf_counter()-start)*1000
 def chain():
  outputs={}
  for name,s,feed,binding,device in sessions:
   if binding is None:values=s.run(None,feed)
   else:
    # Preserve transfer costs: upload each input every iteration, even though the fixture repeats.
    for key,value in feed.items():device[key].update_inplace(value)
    s.run_with_iobinding(binding);values=binding.copy_outputs_to_cpu()
   for info,value in zip(s.get_outputs(),values): outputs[name+':'+info.name]=value
  return outputs
 start=time.perf_counter();outputs=chain();first=(time.perf_counter()-start)*1000
 if verify:
  placements={}
  for name,s,*_ in sessions:
   events=json.loads(Path(s.end_profiling()).read_text());counts=collections.Counter(e.get('args',{}).get('provider') for e in events if e.get('args',{}).get('provider'));placements[name]=dict(counts)
   if config!='CPU':assert counts['CUDAExecutionProvider']>0
  if config=='CPU':np.savez(BASE/f'reference-{batch}.npz',**outputs)
  reference=np.load(BASE/f'reference-{batch}.npz')
  close=all(np.allclose(value,reference[key],rtol=1e-3,atol=1e-3) for key,value in outputs.items())
  diff=max(float(np.max(np.abs(value-reference[key]))) for key,value in outputs.items())
  assert close,(config,batch,diff)
  print(json.dumps({'config':config,'faces':batch,'outputs_close':close,'max_abs_difference':diff,'placements':placements}),flush=True);sys.exit()
 for _ in range(5):chain()
 warmed=rss();times=[]
 for _ in range(int(os.environ.get('OPENCULL_BENCH_ITERATIONS','60'))):
  start=time.perf_counter();chain();times.append((time.perf_counter()-start)*1000)
 print(json.dumps({'config':config,'faces':batch,'baseline_rss_mib':baseline,'warmed_rss_mib':warmed,'peak_rss_mib':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss/1024,'setup_ms':setup_ms,'first_chain_ms':first,'median_ms':float(np.median(times)),'p95_ms':float(np.percentile(times,95)),'samples_ms':times}),flush=True);sys.exit()
import argparse
import numpy as np
from PIL import Image, ImageOps
parser=argparse.ArgumentParser(description='Compare CPU/CUDA inference speed and RAM/VRAM without modifying photos.')
parser.add_argument('--image', required=True, type=Path, help='A local JPEG photo, read only')
parser.add_argument('--output', type=Path, default=BASE)
parser.add_argument('--trials', type=int, default=3)
parser.add_argument('--iterations', type=int, default=60)
parser.add_argument('--all-variants', action='store_true', help='Include workspace/algorithm and explicit device-input variants')
args=parser.parse_args()
if args.trials<1 or args.iterations<1: parser.error('trials and iterations must be positive')
BASE=args.output.resolve();BASE.mkdir(parents=True,exist_ok=True)
os.environ['OPENCULL_BENCH_DIR']=str(BASE)
os.environ['OPENCULL_BENCH_ITERATIONS']=str(args.iterations)
if not args.all_variants:CONFIGS={k:v for k,v in CONFIGS.items() if k in ['CPU','CUDA-default','CUDA-arena-only']}
image=ImageOps.exif_transpose(Image.open(args.image)).convert('RGB')
scale=min(1,2048/max(image.size));w,h=[max(1,round(n*scale)) for n in image.size]
image=np.asarray(image.resize((w,h)),dtype=np.float32)
pw,ph=(w+31)//32*32,(h+31)//32*32
finder=np.zeros((1,3,ph,pw),np.float32);finder[0,:,:h,:w]=image[:,:,::-1].transpose(2,0,1)
rng=np.random.default_rng(20261016)
for n in [1,8]:
 arrays={'finder:input':finder,'judge:face':rng.uniform(0,255,(n,3,224,224)).astype(np.float32),'judge:eyes':rng.uniform(0,255,(n,3,64,224)).astype(np.float32),'eyes:pair':rng.uniform(0,255,(n,3,64,192)).astype(np.float32),'eyes:turn':np.zeros((n,1),np.float32),'mouth:band':rng.uniform(0,255,(n,3,64,128)).astype(np.float32)}
 np.savez(BASE/f'inputs-{n}.npz',**arrays)

report={'configs':CONFIGS,'method':'Fresh processes per configuration/scenario/trial (counts recorded separately); one worker holds four sessions. 5 warmups after first inference, then the requested number of timed four-model chains. CPU intra/inter threads=1; TF32 off. One finder photo plus synthetic face batches of 1 or 8. Identical fixtures from earlier memory benchmark. Default already reuses host buffers. Reusable device inputs use OrtValue.update_inplace on EVERY run to retain upload costs; all outputs copied to CPU. Peak RAM Linux RSS high-water; peak VRAM per PID sampled ~100ms including startup. CPU baseline then configuration order rotated/reversed across trials; background desktop load uncontrolled. Python ORT1.28.0, not end-to-end Rust application.','verification':[],'runs':[]}
report['trials']=args.trials;report['iterations']=args.iterations;report['finder_shape']=list(finder.shape)
# Untimed correctness/placement validation in separate processes.
for batch in [1,8]:
 for config in CONFIGS:
  q=subprocess.run([PYTHON,__file__,'verify',config,str(batch)],capture_output=True,text=True)
  if q.returncode:raise RuntimeError(q.stderr)
  report['verification'].append(json.loads(q.stdout));print('verified '+config+' faces='+str(batch),flush=True)
(BASE/'tuning-results.json').write_text(json.dumps(report,indent=2))
for batch in [1,8]:
 for trial in range(args.trials):
  order=list(CONFIGS)
  if trial==1:order=order[::-1]
  elif trial==2:order=order[3:]+order[:3]
  for config in order:
   p=subprocess.Popen([PYTHON,__file__,'worker',config,str(batch)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   stop=threading.Event();gpu=[]
   def monitor():
    while not stop.is_set():
     q=subprocess.run(['nvidia-smi','--query-compute-apps=pid,used_gpu_memory','--format=csv,noheader,nounits'],capture_output=True,text=True)
     for line in q.stdout.splitlines():
      parts=line.split(',')
      if len(parts)==2 and parts[0].strip()==str(p.pid):
       try:gpu.append(float(parts[1]))
       except ValueError:pass
     stop.wait(.1)
   t=threading.Thread(target=monitor);t.start();out,err=p.communicate();stop.set();t.join()
   if p.returncode:raise RuntimeError(err)
   entry=json.loads(out);entry.update(trial=trial+1,peak_vram_mib=max(gpu,default=0),vram_samples=gpu)
   report['runs'].append(entry);(BASE/'tuning-results.json').write_text(json.dumps(report,indent=2))
   print(f"{config} faces={batch} trial={trial+1}: {entry['median_ms']:.2f}ms peakRAM={entry['peak_rss_mib']:.1f}MiB peakVRAM={entry['peak_vram_mib']:.0f}MiB",flush=True)
print('DONE',flush=True)
