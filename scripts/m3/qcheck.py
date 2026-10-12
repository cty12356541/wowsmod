import sys; from PIL import Image
im=Image.open(sys.argv[1]).convert('RGB').resize((1280,720))
# 两个青色按钮(1280x720 缩略图坐标), 以及按钮之间的非按钮区域
def mean(b):
    c=im.crop(b); px=list(c.getdata()); return [sum(p[i] for p in px)/len(px) for i in range(3)]
yes=mean((556,350,628,364)); no=mean((652,350,724,364))
teal=lambda m: m[1]>90 and m[2]>90 and m[0]<90
print('yes',[round(x) for x in yes],'no',[round(x) for x in no],'OK' if teal(yes) and teal(no) else 'MISMATCH')
