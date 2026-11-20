#!/usr/bin/env python3
"""Build the supplemental report; this does not edit the unavailable whitepaper.

Dependencies: pandoc, reportlab, pymupdf, matplotlib, pillow.
PDF mathematical expressions are rendered at 330 dpi; searchable MathML HTML and
editable LaTeX are provided alongside the PDF. No external browser is required.
"""
from pathlib import Path
import hashlib, html, io, json, re, subprocess, sys
import fitz
import matplotlib
matplotlib.use('Agg')
from matplotlib.mathtext import math_to_image
from matplotlib.font_manager import FontProperties
from PIL import Image as PILImage
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.lib import colors
from reportlab.lib.enums import TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.pdfgen.canvas import Canvas
from reportlab.platypus import (SimpleDocTemplate, Paragraph, Spacer, Image,
                                LongTable, TableStyle, KeepTogether, PageBreak)

ROOT=Path(__file__).resolve().parent
SOURCE=ROOT/'tmp/an1_manuscript.txt'
if not SOURCE.exists(): SOURCE=ROOT/'an1_manuscript_source.txt'
source_text=SOURCE.read_text()
if any(ord(ch)<32 and ch not in '\n\r' for ch in source_text):
    raise ValueError('Unexpected control character in manuscript')
OUT=ROOT/'output/pdf'; OUT.mkdir(parents=True,exist_ok=True)
TMP=ROOT/'tmp/pdfs_an1'; TMP.mkdir(parents=True,exist_ok=True)
STEM='PALW_AN1_whitepaper_revision_v04_20261118'
AST=json.loads(subprocess.run(['pandoc','-f','markdown+tex_math_dollars+raw_tex',
                              '-t','json',str(SOURCE)],capture_output=True,text=True,
                             check=True).stdout)

fontpath=TMP/'Japanese.ttf'
if not fontpath.exists():fontpath.write_bytes(fitz.Font('japan').buffer)
pdfmetrics.registerFont(TTFont('JP',str(fontpath)))
fontroot=Path('/usr/share/fonts/truetype/dejavu')
pdfmetrics.registerFont(TTFont('Latin',str(fontroot/'DejaVuSans.ttf')))
pdfmetrics.registerFont(TTFont('LatinBold',str(fontroot/'DejaVuSans-Bold.ttf')))
pdfmetrics.registerFont(TTFont('Mono',str(fontroot/'DejaVuSansMono.ttf')))
pdfmetrics.registerFontFamily('JP',normal='JP',bold='JP',italic='JP',boldItalic='JP')

NAVY=colors.HexColor('#17354b'); TEAL=colors.HexColor('#177b86')
INK=colors.HexColor('#21303c'); MUTED=colors.HexColor('#5c6a76')
LIGHT=colors.HexColor('#eef4f6'); LINE=colors.HexColor('#d7e2e7')
PAGE_W,PAGE_H=A4; LEFT=21*mm; RIGHT=21*mm
WIDTH=PAGE_W-LEFT-RIGHT

styles={
 'body':ParagraphStyle('Body',fontName='JP',fontSize=9.5,leading=16.0,
      textColor=INK,spaceAfter=7,wordWrap='CJK',splitLongWords=True,
      allowWidows=0,allowOrphans=0),
 'h1':ParagraphStyle('H1',fontName='JP',fontSize=14.5,leading=21,
      textColor=NAVY,spaceBefore=17,spaceAfter=9,keepWithNext=True,
      wordWrap='CJK'),
 'h2':ParagraphStyle('H2',fontName='JP',fontSize=11.2,leading=17,
      textColor=TEAL,spaceBefore=10,spaceAfter=6,keepWithNext=True,
      wordWrap='CJK'),
 'cell':ParagraphStyle('Cell',fontName='JP',fontSize=8.6,leading=13.7,
      textColor=INK,wordWrap='CJK',spaceAfter=0),
 'cellhead':ParagraphStyle('CellHead',fontName='JP',fontSize=8.8,leading=14,
      textColor=NAVY,wordWrap='CJK'),
 'bullet':ParagraphStyle('Bullet',fontName='JP',fontSize=9.5,leading=16,
      textColor=INK,spaceAfter=6,leftIndent=10,firstLineIndent=0,
      bulletIndent=0,wordWrap='CJK',allowWidows=0,allowOrphans=0),
}

MATH={}; math_failures=[]
def formula(tex,display=False):
    # Equivalent spellings required by matplotlib's restricted TeX parser.
    tex=re.sub(r'\\ge(?![A-Za-z])',r'\\geq',tex)
    tex=re.sub(r'\\le(?![A-Za-z])',r'\\leq',tex)
    tex=re.sub(r'\\mathbb\s+([A-Za-z])',lambda m:r'\mathbb{'+m.group(1)+'}',tex)
    tex=re.sub(r'\\mathcal\s+([A-Za-z])',lambda m:r'\mathcal{'+m.group(1)+'}',tex)
    tex=tex.replace(r'\frac4q',r'\frac{4}{q}').replace(r'\frac dr',r'\frac{d}{r}')
    tex=tex.replace(r'\binom L2',r'\binom{L}{2}')
    tex=tex.replace(r'\land',r'\wedge').replace(r'\lor',r'\vee')
    tex=re.sub(r'\\pmod\s+([A-Za-z])',lambda m:r'\;(\mathrm{mod}\ '+m.group(1)+')',tex)
    size=12.8 if display else 10.0
    key=hashlib.sha256((str(size)+'|'+tex).encode()).hexdigest()[:18]
    if key in MATH:return MATH[key]
    path=TMP/('math_'+key+'.png')
    try:
        math_to_image('$'+tex+'$',str(path),prop=FontProperties(size=size),
                      dpi=330,format='png',color='#203343')
    except Exception as e:
        math_failures.append({'tex':tex,'error':str(e)})
        raise
    with PILImage.open(path) as im:
        width,height=im.size
    width,height=width*72/330,height*72/330
    if display and width>WIDTH-8:
        scale=(WIDTH-8)/width; width*=scale;height*=scale
    item=(path,width,height)
    MATH[key]=item
    return item

def inline(nodes):
    parts=[]
    for node in nodes:
        t=node['t']; c=node.get('c')
        if t=='Str':parts.append(html.escape(c))
        elif t in ('Space','SoftBreak'):parts.append(' ')
        elif t=='LineBreak':parts.append('<br/>')
        elif t=='Strong':parts.append('<font color="#17354b">'+inline(c)+'</font>')
        elif t=='Emph':parts.append(inline(c))
        elif t=='Code':parts.append('<font name="Mono" size="8.3">'+html.escape(c[1])+'</font>')
        elif t=='Math':
            path,w,h=formula(c[1],False)
            parts.append(f'<img src="{path}" width="{w:.3f}" height="{h:.3f}" valign="middle"/>')
        elif t=='Link':
            parts.append('<link href="'+html.escape(c[2][0],quote=True)+'" color="#177b86">'+inline(c[1])+'</link>')
        elif t=='Quoted':parts.append('「'+inline(c[1])+'」')
        elif t=='Span':parts.append(inline(c[1]))
        elif t=='RawInline':
            if c[0]=='html':parts.append(c[1])
        else: raise ValueError(('unsupported inline',t,c))
    return ''.join(parts)

def par(nodes,style='body',**kw):
    markup=inline(nodes)
    heights=[float(h) for h in re.findall(r'height="([0-9.]+)"',markup)]
    s=styles[style]
    if heights:
        # ReportLab's CJK fragment splitter does not support inline images.
        # Its ordinary splitter can wrap long Japanese text and math fragments.
        s=ParagraphStyle(s.name+'Math',parent=s,leading=max(s.leading,max(heights)+2),
                         wordWrap=None,splitLongWords=True)
    return Paragraph(markup,s,**kw)

def textplain(nodes):
    return ''.join(x.get('c','') if x['t']=='Str' else ' ' if x['t']=='Space' else '' for x in nodes)

def cell_content(blocks,header=False):
    out=[]
    for b in blocks:
        if b['t'] in ('Plain','Para'):out.append(par(b['c'],'cellhead' if header else 'cell'))
        else: out.extend(blocks_to_flowables([b]))
    return out

def render_table(c):
    cols=len(c[2]); rows=[]
    headrows=c[3][1]
    bodyrows=[]
    for body in c[4]:bodyrows.extend(body[2]+body[3])
    for ishead, rowgroup in [(True,headrows),(False,bodyrows)]:
        for row in rowgroup:
            rows.append([cell_content(cell[4],ishead) for cell in row[1]])
    if cols==2:
        first=''.join(textplain(b['c']) for b in headrows[0][1][0][4] if b['t'] in ('Plain','Para'))
        ratio=.40
        if first=='追加資料の節':ratio=.26
        if '検証グループ' in first:ratio=.80
        if first=='項目':ratio=.52
        widths=[WIDTH*ratio,WIDTH*(1-ratio)]
    elif cols==3:widths=[WIDTH*.22,WIDTH*.39,WIDTH*.39]
    else:widths=[WIDTH/cols]*cols
    table=LongTable(rows,colWidths=widths,repeatRows=len(headrows),
                    hAlign='LEFT',spaceBefore=5,spaceAfter=10)
    cmds=[('VALIGN',(0,0),(-1,-1),'TOP'),
          ('LEFTPADDING',(0,0),(-1,-1),7),('RIGHTPADDING',(0,0),(-1,-1),7),
          ('TOPPADDING',(0,0),(-1,-1),6),('BOTTOMPADDING',(0,0),(-1,-1),6),
          ('BACKGROUND',(0,0),(-1,len(headrows)-1),LIGHT),
          ('LINEBELOW',(0,0),(-1,len(headrows)-1),.7,TEAL),
          ('LINEBELOW',(0,len(rows)-1),(-1,len(rows)-1),.6,LINE)]
    for r in range(len(headrows),len(rows)):
        if (r-len(headrows))%2==1:cmds.append(('BACKGROUND',(0,r),(-1,r),colors.HexColor('#f8fafb')))
        cmds.append(('LINEBELOW',(0,r),(-1,r),.3,LINE))
    table.setStyle(TableStyle(cmds))
    if len(rows)<=5:
        return KeepTogether([table])
    return table

def blocks_to_flowables(blocks):
    out=[]
    for block in blocks:
        t=block['t'];c=block.get('c')
        if t=='Header':
            level,attrs,nodes=c
            heading=par(nodes,'h1' if level==1 else 'h2')
            out.append(heading)
        elif t in ('Para','Plain'):
            if len(c)==1 and c[0]['t']=='Math' and c[0]['c'][0]['t']=='DisplayMath':
                path,w,h=formula(c[0]['c'][1],True)
                img=Image(str(path),width=w,height=h,hAlign='CENTER')
                out.append(KeepTogether([Spacer(1,3),img,Spacer(1,9)]))
            else:out.append(par(c))
        elif t=='Table':out.append(render_table(c))
        elif t=='BulletList':
            for item in c:
                for i,b in enumerate(item):
                    if b['t'] in ('Plain','Para'):out.append(par(b['c'],'bullet',bulletText='•' if i==0 else None))
                    else:out.extend(blocks_to_flowables([b]))
        elif t=='OrderedList':
            for num,item in enumerate(c[1],c[0][0]):
                for i,b in enumerate(item):
                    if b['t'] in ('Plain','Para'):out.append(par(b['c'],'bullet',bulletText=f'{num}.' if i==0 else None))
                    else:out.extend(blocks_to_flowables([b]))
        elif t=='RawBlock':
            if 'newpage' in c[1]:out.append(PageBreak())
        elif t=='HorizontalRule':out.append(Spacer(1,10))
        elif t=='Div':out.extend(blocks_to_flowables(c[1]))
        else:raise ValueError(('unsupported block',t))
    return out

class NumberedCanvas(Canvas):
    def __init__(self,*args,**kwargs):
        super().__init__(*args,**kwargs);self.saved=[]
    def showPage(self):
        self.saved.append(dict(self.__dict__));self._startPage()
    def save(self):
        total=len(self.saved)
        for state in self.saved:
            self.__dict__.update(state);self.decoration(total);super().showPage()
        super().save()
    def decoration(self,total):
        self.saveState()
        self.setStrokeColor(LINE);self.setLineWidth(.5)
        self.line(LEFT,PAGE_H-14.7*mm,PAGE_W-RIGHT,PAGE_H-14.7*mm)
        self.setFont('Latin',7.2);self.setFillColor(MUTED)
        self.drawString(LEFT,PAGE_H-12*mm,'PALW-AN1  |  PANEL + COURT  |  DRAFT v0.4')
        self.setFont('JP',7.2)
        self.drawString(LEFT,12*mm,'AN1改訂統合稿 / DRAFT_NOT_ACTIVATED / 2026-11-18')
        self.setFont('Latin',7.4)
        self.drawRightString(PAGE_W-RIGHT,12*mm,f'{self._pageNumber} / {total}')
        self.restoreState()

story=[]
story.append(Paragraph('RESEARCH REVIEW  /  2026.11.18',ParagraphStyle('eyebrow',fontName='LatinBold',fontSize=8.8,leading=13,textColor=TEAL,spaceAfter=11)))
story.append(Paragraph('PALW-AN1<br/>Whitepaper 改訂統合稿',ParagraphStyle('title',fontName='JP',fontSize=23,leading=33,textColor=NAVY,spaceAfter=12)))
story.append(Paragraph('実入力への束縛・完全再実行・局所反証 / v0.4',ParagraphStyle('sub',fontName='JP',fontSize=11,leading=18,textColor=MUTED,spaceAfter=15)))
story+=blocks_to_flowables(AST['blocks'])

pdf=OUT/(STEM+'.pdf')
doc=SimpleDocTemplate(str(pdf),pagesize=A4,leftMargin=LEFT,rightMargin=RIGHT,
 topMargin=22*mm,bottomMargin=21*mm,title='PALW-AN1 Whitepaper 改訂統合稿 v0.4',
 author='Technical review prepared for the user',subject='PALW-AN1 Panel and Court revision; original whitepaper source unavailable; RFC archive reviewed')
doc.build(story,canvasmaker=NumberedCanvas)

css='''body{max-width:850px;margin:40px auto;padding:0 22px;color:#21303c;font-family:"Noto Serif CJK JP",serif;line-height:1.85}h1,h2{font-family:"Noto Sans CJK JP",sans-serif;color:#17354b;line-height:1.4}h1{margin-top:2.3em}h2{color:#177b86}table{border-collapse:collapse;width:100%;font-size:.94em;margin:1em 0}th,td{border-bottom:1px solid #d7e2e7;padding:.6em;vertical-align:top}th{background:#eef4f6}tr:nth-child(even){background:#f8fafb}a{color:#177b86}math{font-family:"Latin Modern Math","STIX Two Math",math}math[display=block]{overflow-x:auto;padding:12px 0}code{overflow-wrap:anywhere}header{border-bottom:3px solid #177b86;padding-bottom:1em}.subtitle,.date{color:#5c6a76}@media print{body{margin:0;max-width:none}h1,h2{break-after:avoid}tr,math{break-inside:avoid}}'''
csspath=TMP/'style.html';csspath.write_text('<style>'+css+'</style>')
subprocess.run(['pandoc',str(SOURCE),'-f','markdown+tex_math_dollars+raw_tex','-t','html5',
                '--standalone','--mathml','--include-in-header',str(csspath),
                '-o',str(OUT/(STEM+'.html'))],check=True)
subprocess.run(['pandoc',str(SOURCE),'-f','markdown+tex_math_dollars+raw_tex','-t','latex',
 '--standalone','--pdf-engine=xelatex','-V','mainfont=DejaVu Serif',
 '-V','CJKmainfont=Noto Serif CJK JP','-V','geometry:margin=22mm','-V','fontsize=10pt',
 '-o',str(OUT/(STEM+'.tex'))],check=True)

with fitz.open(pdf) as p:
    summary={'pages':len(p),'math_expressions':len(MATH),'math_render_failures':math_failures,
             'text_characters':sum(len(page.get_text()) for page in p),'pdf_size_bytes':pdf.stat().st_size,
             'original_whitepaper_integrated':False,
             'source_sha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest()}
(TMP/'build_summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2))
print(json.dumps(summary,ensure_ascii=False))
