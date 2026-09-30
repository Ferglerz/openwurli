#!/usr/bin/env python3
"""Bounded spectral follow-up of rejected cpu-next renders; never changes samples."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import sys
import numpy as np


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def db(value):
    return 20 * math.log10(value) if value > 0 and math.isfinite(value) else None


def metrics(a, b):
    d=b-a
    rms=lambda x:float(np.sqrt(np.mean(x*x)))
    reference_rms=rms(a);residual_rms=rms(d)
    return {'samples':len(a),'reference_peak':float(np.max(np.abs(a))),'candidate_peak':float(np.max(np.abs(b))),
        'residual_peak':float(np.max(np.abs(d))),'residual_peak_dbfs':db(float(np.max(np.abs(d)))),
        'reference_rms_dbfs':db(reference_rms),'candidate_rms_dbfs':db(rms(b)),
        'residual_rms_dbfs':db(residual_rms),'residual_relative_db':db(residual_rms/reference_rms) if reference_rms else None,
        'zero_reference':reference_rms==0,'zero_reference_violation':reference_rms==0 and residual_rms!=0}


def band_spectrum(a,b,sr):
    # Rectangular DFT + Parseval accounting: sum of one-sided power equals mean square.
    n=len(a);f=np.fft.rfftfreq(n,1/sr)
    weight=np.full(len(f),2.0);weight[0]=1
    if n%2==0:weight[-1]=1
    power=lambda x:np.abs(np.fft.rfft(x))**2*weight/(n*n)
    powers=[power(x) for x in [a,b,b-a]]
    boundaries=[0,20,200,2000,10000,sr/2+1e-9];bands=[]
    total=float(np.sum(powers[2]))
    for low,high in zip(boundaries,boundaries[1:]):
        mask=(f>=low)&(f<high);r,c,d=[float(np.sum(p[mask])) for p in powers]
        bands.append({'low_hz':low,'high_hz':min(high,sr/2),'reference_band_rms_dbfs':db(math.sqrt(r)),
            'candidate_band_rms_dbfs':db(math.sqrt(c)),'residual_band_rms_dbfs':db(math.sqrt(d)),
            'residual_relative_to_reference_band_db':db(math.sqrt(d/r)) if r else None,
            'residual_energy_fraction':d/total if total else 0.0})
    return {'method':'Rectangular one-sided DFT, no detrend; band-integrated RMS by Parseval, full-scale amplitude=1; spectral leakage possible.',
        'bin_spacing_hz':sr/n,'bands':bands,'parseval_residual_rms_dbfs':db(math.sqrt(total))}


def display_spectrum(x,sr):
    window=np.hanning(len(x));scale=float(np.sum(window))
    spectrum=2*np.abs(np.fft.rfft(x*window))/scale
    spectrum[0]*=.5
    if len(x)%2==0:spectrum[-1]*=.5
    return np.fft.rfftfreq(len(x),1/sr),20*np.log10(np.maximum(spectrum,1e-30))


def sliding_rows(a,b,sr,limits):
    rows=[];length=sr*20//1000;hop=sr*10//1000
    for start in range(0,len(a),hop):
        end=min(start+length,len(a));m=metrics(a[start:end],b[start:end]);relative=m['residual_relative_db']
        peak_failure=(m['residual_peak_dbfs'] is not None and m['residual_peak_dbfs']>limits['peak_dbfs'])
        relative_failure=m['zero_reference_violation'] or (relative is not None and relative>limits['rms_relative_db'])
        rows.append({'start_sample':start,'end_sample':end,'start_seconds':start/sr,'end_seconds':end/sr,
            'peak_gate_failed':peak_failure,'relative_gate_failed':relative_failure,**m})
    return rows


def event_times(case,sr):
    return [{'sample':at,'seconds':at/sr,'event':ev} for at,ev in case['events']]


def analyze_case(directory,case):
    if len(case['rate_segments'])!=1:raise ValueError('This bounded follow-up accepts fixed-rate selected cases only.')
    sr=case['scenario']['sample_rate'];paths=[directory/p for p in case['waveforms']]
    raw=[np.fromfile(p,dtype='<f4') for p in paths]
    if len(raw[0])!=len(raw[1]) or not all(np.all(np.isfinite(x)) for x in raw):raise ValueError('Invalid paired input')
    a,b=[x.astype(np.float64) for x in raw];d=b-a
    if len(a)!=case['comparison']['full']['samples']:raise ValueError('Recorded sample count differs')
    peak=int(np.argmax(np.abs(d)));events=event_times(case,sr)
    onsets=[e['sample'] for e in events if isinstance(e['event'],dict) and 'On' in e['event']]
    offs=[e['sample'] for e in events if isinstance(e['event'],dict) and 'Off' in e['event']]
    pedals=[e['sample'] for e in events if isinstance(e['event'],dict) and e['event'].get('Sustain') is False]
    windows=[('first_attack_60ms',0,min(len(a),int(.06*sr))),
        ('largest_residual_20ms',max(0,peak-sr//100),min(len(a),peak+sr//100)),
        ('final_200ms',max(0,len(a)-sr//5),len(a))]
    if offs:windows.append(('first_note_off_60ms',offs[0],min(len(a),offs[0]+int(.06*sr))))
    if pedals:windows.append(('first_pedal_release_60ms',pedals[0],min(len(a),pedals[0]+int(.06*sr))))
    if offs or pedals:
        last=max(offs+pedals);windows.append(('last_release_60ms',last,min(len(a),last+int(.06*sr))))
    worst=case['comparison']['sliding_20ms']['worst_relative']
    windows.append(('worst_relative_gate_window',worst['start_sample'],worst['end_sample']))
    result={'scenario':case['scenario'],'input_sha256':{str(p.relative_to(directory)):sha(p) for p in paths},
        'samples':len(a),'events':events,'peak_residual_sample':peak,'peak_residual_seconds':peak/sr,
        'first_bit_mismatch_sample':case['comparison']['full']['first_mismatch_sample'],
        'full':metrics(a,b),'full_spectrum':band_spectrum(a,b,sr),'windows':[]}
    for label,start,end in windows:
        if end>start:
            result['windows'].append({'label':label,'start_sample':start,'end_sample':end,'start_seconds':start/sr,'end_seconds':end/sr,
                'metrics':metrics(a[start:end],b[start:end]),'spectrum':band_spectrum(a[start:end],b[start:end],sr)})
    return result,(a,b,sr,peak)


def plot(path,cases,engine_label):
    # Pillow is available in the bundled runtime; no plotting dependency is installed.
    from PIL import Image,ImageDraw,ImageFont
    height=210+405*len(cases)
    image=Image.new('RGB',(1600,height),'#f9fafb');draw=ImageDraw.Draw(image)
    try:
        font=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf',18)
        title=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial Bold.ttf',25)
        small=ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf',15)
    except OSError:font=title=small=ImageFont.load_default()
    draw.text((55,22),f'Rejected {engine_label}-mode damper recurrence: selected residuals',fill='#111827',font=title)
    draw.text((55,62),'Original sample alignment and gain. No normalization, offset correction, clipping or filtering.',fill='#374151',font=font)
    colors=['#2563eb','#d97706','#dc2626'];labels=['Native reference','Candidate','Candidate minus reference']
    for i,(color,label) in enumerate(zip(colors,labels)):
        x=55+i*350;draw.line((x,103,x+30,103),fill=color,width=3);draw.text((x+40,90),label,fill='#111827',font=font)
    def axes(box,xvalues,yvalues,xlab,ylab,logx=False):
        x0,y0,x1,y1=box;draw.rectangle(box,outline='#9ca3af',width=1)
        def point(x,y):
            fx=(math.log10(x)-math.log10(xvalues[0]))/(math.log10(xvalues[-1])-math.log10(xvalues[0])) if logx else (x-xvalues[0])/(xvalues[-1]-xvalues[0])
            fy=(y-yvalues[0])/(yvalues[-1]-yvalues[0]);return x0+fx*(x1-x0),y1-fy*(y1-y0)
        for x in xvalues:
            px,_=point(x,yvalues[0]);draw.line((px,y0,px,y1),fill='#e5e7eb');draw.text((px-18,y1+7),f'{x:g}',fill='#374151',font=small)
        for y in yvalues:
            _,py=point(xvalues[0],y);draw.line((x0,py,x1,py),fill='#e5e7eb');draw.text((x0-58,py-9),f'{y:g}',fill='#374151',font=small)
        draw.text((x0+160,y1+34),xlab,fill='#374151',font=font);draw.text((x0,y0-26),ylab,fill='#374151',font=small)
        return point
    for row,(name,(a,b,sr,peak)) in enumerate(cases):
        top=200+row*405;draw.text((65,top-63),name,fill='#111827',font=font)
        half=int(sr*.015);start=max(0,peak-half);end=min(len(a),peak+half)
        xs=(np.arange(start,end)-peak)/sr*1000
        extent=max(float(np.max(np.abs(x[start:end]))) for x in [a,b,b-a]);ylim=math.ceil(extent)
        point=axes((100,top,755,top+280),[-15,-10,-5,0,5,10,15],[-ylim,-ylim/2,0,ylim/2,ylim],'Time relative to peak residual (ms)','Raw output amplitude (1 = 0 dBFS)')
        for data,color in zip([a,b,b-a],colors):
            draw.line([point(float(x),float(y)) for x,y in zip(xs,data[start:end])],fill=color,width=2)
        point=axes((910,top,1535,top+280),[20,100,1000,10000,24000],[-160,-120,-80,-40,0,20],'Frequency (Hz, logarithmic)','Peak-centered 20 ms Hann FFT amplitude (dBFS)',True)
        for data,color in zip([a,b,b-a],colors):
            spectral_start=max(0,peak-sr//100);spectral_end=min(len(data),peak+sr//100)
            frequencies,amplitudes=display_spectrum(data[spectral_start:spectral_end],sr);edges=np.geomspace(20,sr/2,600);points=[]
            for low,high in zip(edges,edges[1:]):
                selected=(frequencies>=low)&(frequencies<high)
                if np.any(selected):points.append(point(math.sqrt(low*high),float(np.clip(np.max(amplitudes[selected]),-160,20))))
            if len(points)>1:draw.line(points,fill=color,width=2)
    draw.text((55,height-35),'Spectrum: Hann window; peak per logarithmic display bin. JSON band powers use unsmoothed rectangular-DFT Parseval accounting.',fill='#4b5563',font=small)
    image.save(path)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('run',type=Path);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    if args.output.exists() and any(args.output.iterdir()):raise SystemExit('Use an empty output directory.')
    args.output.mkdir(parents=True,exist_ok=True)
    report=json.loads((args.run/'report.json').read_text());provenance=json.loads((args.run/'provenance.json').read_text())
    failures=[case for case in report['audio'] if not case['comparison']['within_bounds']]
    failing=[];selected=[];plots=[]
    for case in failures:
        a=np.fromfile(args.run/case['waveforms'][0],dtype='<f4').astype(np.float64)
        b=np.fromfile(args.run/case['waveforms'][1],dtype='<f4').astype(np.float64)
        sr=case['scenario']['sample_rate'];rows=sliding_rows(a,b,sr,report['limits'])
        bad=[w for w in rows if w['peak_gate_failed'] or w['relative_gate_failed']]
        if len(bad)!=case['comparison']['sliding_20ms']['failed_windows']:raise ValueError('Window count does not match harness: '+case['scenario']['name'])
        whole=metrics(a,b)
        failing.append({'name':case['scenario']['name'],'full':whole,'failed_windows':len(bad),'input_sha256':{relative:sha(args.run/relative) for relative in case['waveforms']},
            'peak_failing_windows':sum(w['peak_gate_failed'] for w in bad),'relative_only_windows':sum(not w['peak_gate_failed'] and w['relative_gate_failed'] for w in bad),
            'maximum_residual_rms_dbfs_in_failed_window':max(w['residual_rms_dbfs'] if w['residual_rms_dbfs'] is not None else -math.inf for w in bad),
            'failed_window_intervals_seconds':[[w['start_seconds'],w['end_seconds']] for w in bad]})
        prefixes=['pedal-bursts-6-','release-long-6-','repeat-chord-6-'];strong=any(case['scenario']['name'].startswith(p) for p in prefixes)
        if strong or case['scenario']['name']=='long-release-tail':
            result,signals=analyze_case(args.run,case);selected.append(result)
            if strong:plots.append((case['scenario']['name'],signals))
    result={'schema':1,'source_run':str(args.run.resolve()),'input_report_sha256':sha(args.run/'report.json'),'input_provenance_sha256':sha(args.run/'provenance.json'),
        'analysis_script_sha256':sha(Path(__file__)),'python':platform.python_version(),'numpy':np.__version__,
        'original_source_unchanged_during_run':provenance['source_unchanged_during_run'],'original_audio_scenarios':report['audio_scenarios'],
        'failed_cases':len(failures),'failed_windows':sum(c['failed_windows'] for c in failing),
        'cases_failing_whole_render':sum(not case['comparison']['full']['within_bounds'] for case in failures),
        'analysis_transforms':'No gain matching, alignment, offset/DC removal or filtering. Residual is candidate-reference in float64 from exact input f32. Hann window is used only for display spectra; numeric band powers use unwindowed Parseval accounting.',
        'interpretation_limits':'No listening test, psychoacoustic weighting, harmonic-isolation or alias classifier. Relative errors near silent tails can be large despite tiny absolute residual. Large transient failures independently reject this candidate.',
        'failures':failing,'selected_cases':selected}
    (args.output/'analysis.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    plot(args.output/'residual-analysis.png',plots,report['candidate_initial_mode'].capitalize())
    (args.output/'analysis-script.py').write_bytes(Path(__file__).read_bytes())
    print(json.dumps({k:result[k] for k in ['original_audio_scenarios','failed_cases','failed_windows','cases_failing_whole_render']},indent=2))
    print(args.output/'analysis.json');print(args.output/'residual-analysis.png')


if __name__=='__main__':main()
