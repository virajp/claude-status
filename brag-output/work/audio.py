import math,random,wave,array
SR=44100;DUR=21.5;N=int(SR*DUR)
music=[0.0]*N;sfx=[0.0]*N
def f(m):return 440*2**((m-69)/12)
def add(buf,t0,dur,fn,gain):
    a=int(t0*SR);b=min(N,int((t0+dur)*SR))
    for i in range(max(a,0),b):buf[i]+=gain*fn((i-a)/SR)
BPM=120;BEAT=60/BPM
# A minor: Am F C G, 2 beats... one chord per bar (4 beats = 2s)
prog=[[57,60,64,69],[53,57,60,65],[48,55,60,64],[55,59,62,67]]
roots=[45,41,48,43]
def pad(freq,L):
    def fn(t):
        env=min(1,t/0.35)*min(1,(L-t)/0.3)
        s=math.sin(2*math.pi*freq*t)+0.35*math.sin(2*math.pi*freq*2.003*t)+0.15*math.sin(2*math.pi*freq*3.01*t)
        return env*s
    return fn
bar=0;t=0.0
while t<18.0:
    ch=prog[bar%4]
    for m in ch: add(music,t,2.05,pad(f(m),2.05),0.035)
    t+=2.0;bar+=1
# final chord: Am(add9) ring out
for m in [45,57,60,64,71,69]:
    add(music,18.0,3.5,lambda x,fr=f(m):min(1,x/0.05)*math.exp(-x*0.9)*(math.sin(2*math.pi*fr*x)+0.3*math.sin(4*math.pi*fr*x)),0.05)
# bass pluck on beats 1 & 3 from 0.4s
def bass(fr):return lambda x:math.exp(-x*5)*math.sin(2*math.pi*fr*x+2*math.exp(-x*20)*math.sin(2*math.pi*fr*x))
k=0
while k*BEAT<18.0:
    tb=k*BEAT
    if k%2==0 and tb>=0.0: add(music,tb,0.9,bass(f(roots[int(tb//2)%4])),0.16)
    k+=1
# soft kick from 3.0 to 18.0, hats throughout
def kick(x):
    ph=2*math.pi*(50*x+60*(1-math.exp(-x*30))/30)
    return math.exp(-x*9)*math.sin(ph)
k=0
while k*BEAT<18.0:
    tb=k*BEAT
    if tb>=3.0 and not(10.6<tb<11.4): add(music,tb,0.35,kick,0.30)
    k+=1
random.seed(3)
noise=[random.uniform(-1,1) for _ in range(SR)]
def hat(x):return math.exp(-x*60)*noise[int(x*SR)%SR]
k=0
while k*BEAT/2<18.0:
    tb=k*BEAT/2
    if tb>=1.0 and k%2==1: add(music,tb,0.08,hat,0.025)
    k+=1
# sfx: soft in-key blips (pentatonic A minor, high)
pent=[81,84,86,88,91,93,96]
def blip(fr):return lambda x:min(1,x/0.004)*math.exp(-x*18)*(math.sin(2*math.pi*fr*x)+0.2*math.sin(4*math.pi*fr*x))
orders=[0,1,2,3,4,5,6]
for o in orders: add(sfx,0.45+[0,1,2,3,4,5,6][o]*0.16 if o<4 else 0.45+(o+0)*0.16,0.4,blip(f(pent[o%7])),0.05)
for k in range(7): add(sfx,7.5+k*0.55,0.3,blip(f(pent[[0,2,1,3,2,4,5][k]])),0.035)
for k in range(7): add(sfx,4.0+k*0.12,0.3,blip(f(pent[k]-12)),0.03)
for k in range(3): add(sfx,15.75+k*0.22,0.4,blip(f([76,79,81][k])),0.05)
# cap hit: swell into 11.3, low boom, tritone-free minor bell
def swell(x):
    return (x/0.7)**2*noise[int(x*SR)%SR]
lp=0.0;a=int(10.6*SR)
for i in range(int(0.7*SR)):
    x=i/SR;lp+=0.04*(swell(x)-lp);sfx[a+i]+=0.12*lp
add(sfx,11.3,1.6,lambda x:math.exp(-x*3)*math.sin(2*math.pi*55*x),0.35)
add(sfx,11.3,1.8,lambda x:math.exp(-x*2.2)*(math.sin(2*math.pi*f(76)*x)+0.5*math.sin(2*math.pi*f(81)*x)),0.05)
# outro shimmer
for k,m in enumerate([81,84,88,93]): add(sfx,18.1+k*0.09,1.2,blip(f(m)),0.035)
# mix, reverb
mix=[music[i]+sfx[i] for i in range(N)]
def comb(x,d,g):
    y=[0.0]*len(x)
    for i in range(len(x)):y[i]=x[i]+(g*y[i-d] if i>=d else 0)
    return y
rv=[0.0]*N
for d in (1557,1617,1491,1422):
    c=comb(mix,d,0.78)
    for i in range(N):rv[i]+=c[i]*0.25
L=[0.0]*N;R=[0.0]*N
for i in range(N):
    wet=rv[i]*0.18
    L[i]=mix[i]+wet;R[i]=mix[i]+ (rv[i-331]*0.18 if i>=331 else 0)
peak=max(max(abs(v) for v in L),max(abs(v) for v in R))
g=0.8/peak
fade0=int(0.02*SR);fadeE=int(1.2*SR)
out=array.array('h')
for i in range(N):
    e=min(1,i/fade0)*min(1,(N-i)/fadeE)
    out.append(int(max(-1,min(1,L[i]*g*e))*32767));out.append(int(max(-1,min(1,R[i]*g*e))*32767))
w=wave.open("music.wav","wb");w.setnchannels(2);w.setsampwidth(2);w.setframerate(SR);w.writeframes(out.tobytes());w.close()
print("peak",peak)
