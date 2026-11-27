"""Create a small deterministic OpenDocument spreadsheet; no spreadsheet engine is used.

The XML is written by hand so cached formula results, repeats, and spans are explicit.
Output is not a LibreOffice-produced file; it follows the patterns LibreOffice writes.
"""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED, ZIP_STORED

NS = (
    'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
    'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
    'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
    'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
    'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" '
    'xmlns:xlink="http://www.w3.org/1999/xlink" '
    'xmlns:dc="http://purl.org/dc/elements/1.1/" '
    'xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0" '
    'xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2" '
    'xmlns:calcext="urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0"'
)

STYLES = f'''<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS} office:version="1.3">
<office:styles>
<style:default-style style:family="table-cell"><style:text-properties style:font-name="Liberation Sans" fo:font-size="10pt"/></style:default-style>
<style:style style:name="Default" style:family="table-cell"/>
<style:style style:name="Heading" style:family="table-cell" style:parent-style-name="Default"><style:text-properties fo:font-weight="bold"/></style:style>
</office:styles>
</office:document-styles>'''

CONTENT = f'''<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS} office:version="1.3">
<office:automatic-styles>
<style:style style:name="co1" style:family="table-column"/>
<style:style style:name="ce1" style:family="table-cell" style:parent-style-name="Heading"/>
<style:style style:name="ce2" style:family="table-cell" style:parent-style-name="Default" style:data-style-name="N10"/>
<style:style style:name="ce3" style:family="table-cell" style:parent-style-name="Default" style:data-style-name="N37"/>
<style:style style:name="ce4" style:family="table-cell" style:parent-style-name="Default"><style:table-cell-properties fo:background-color="#ffff00"/></style:style>
<style:style style:name="ce5" style:family="table-cell" style:parent-style-name="Default"><style:table-cell-properties style:text-align-source="fix"/><style:paragraph-properties fo:text-align="end"/></style:style>
<style:style style:name="T1" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>
</office:automatic-styles>
<office:body><office:spreadsheet>
<table:table table:name="売上">
<table:table-column table:style-name="co1" table:number-columns-repeated="3"/>
<table:table-column table:style-name="co1" table:visibility="collapse"/>
<table:table-column table:style-name="co1" table:number-columns-repeated="1020"/>
<table:table-row>
<table:table-cell table:style-name="ce1" office:value-type="string"><text:p>品目</text:p></table:table-cell>
<table:table-cell table:style-name="ce1" office:value-type="string"><text:p>単価</text:p></table:table-cell>
<table:table-cell table:style-name="ce1" office:value-type="string"><text:p>数量</text:p></table:table-cell>
<table:table-cell table:style-name="ce1" office:value-type="string"><text:p>金額</text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell office:value-type="string"><text:p>りんご</text:p></table:table-cell>
<table:table-cell office:value-type="float" office:value="120"><text:p>120</text:p></table:table-cell>
<table:table-cell office:value-type="float" office:value="30"><text:p>30</text:p></table:table-cell>
<table:table-cell table:formula="of:=[.B2]*[.C2]" office:value-type="float" office:value="3600"><text:p>3600</text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell office:value-type="string"><text:p>みかん</text:p></table:table-cell>
<table:table-cell office:value-type="float" office:value="80"><text:p>80</text:p></table:table-cell>
<table:table-cell office:value-type="float" office:value="20"><text:p>20</text:p></table:table-cell>
<table:table-cell table:formula="of:=[.B3]*[.C3]" office:value-type="float" office:value="1600"><text:p>1600</text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell office:value-type="string"><text:p>合計</text:p></table:table-cell>
<table:table-cell table:number-columns-repeated="2"/>
<table:table-cell table:formula="of:=SUM([.D2:.D3])" office:value-type="float" office:value="5200"><text:p>5200</text:p></table:table-cell>
</table:table-row>
<table:table-row table:number-rows-repeated="1048572"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
</table:table>
<table:table table:name="型と書式">
<table:table-column table:number-columns-repeated="1024"/>
<table:table-row>
<table:table-cell office:value-type="string"><text:p>00123</text:p></table:table-cell>
<table:table-cell office:value-type="boolean" office:boolean-value="true"><text:p>TRUE</text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell table:style-name="ce3" office:value-type="date" office:date-value="2024-01-01"><text:p>2024/01/01</text:p></table:table-cell>
<table:table-cell table:style-name="ce2" office:value-type="percentage" office:value="0.25"><text:p>25%</text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell office:value-type="string"><text:p><text:span text:style-name="T1">重要</text:span>な連絡</text:p></table:table-cell>
<table:table-cell office:value-type="string"><text:p><text:a xlink:href="https://example.com/">Example</text:a></text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell office:value-type="string"><text:p>1行目</text:p><text:p>2行目</text:p></table:table-cell>
<table:table-cell office:value-type="string"><text:p>a<text:s text:c="2"/>b</text:p></table:table-cell>
</table:table-row>
<table:table-row>
<table:table-cell table:number-columns-spanned="2" table:number-rows-spanned="1" office:value-type="string"><text:p>結合</text:p></table:table-cell>
<table:covered-table-cell/>
</table:table-row>
<table:table-row>
<table:table-cell table:formula="of:=[$売上.D4]" office:value-type="float" office:value="5200"><text:p>5200</text:p></table:table-cell>
<table:table-cell table:formula="of:=IF([.A1]=&quot;a;b&quot;;1;2)" office:value-type="float" office:value="2"><text:p>2</text:p></table:table-cell>
</table:table-row>
<table:table-row table:visibility="collapse">
<table:table-cell table:style-name="ce4" office:value-type="string"><office:annotation><text:p>メモ</text:p></office:annotation><text:p>背景色</text:p></table:table-cell>
<table:table-cell table:style-name="ce5" office:value-type="float" office:value="7"><text:p>7</text:p></table:table-cell>
</table:table-row>
<table:table-row table:number-rows-repeated="1048569"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
</table:table>
</office:spreadsheet></office:body>
</office:document-content>'''

MANIFEST = '''<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
<manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>
<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
</manifest:manifest>'''


def entry(name, method):
    """Fixed timestamps keep the archive byte-for-byte reproducible."""
    info = ZipInfo(name, date_time=(2024, 1, 1, 0, 0, 0))
    info.compress_type = method
    return info


def main():
    path = Path(__file__).with_name('workbook.ods')
    with ZipFile(path, 'w') as archive:
        # The mimetype must be the first entry and stored uncompressed.
        archive.writestr(entry('mimetype', ZIP_STORED), 'application/vnd.oasis.opendocument.spreadsheet')
        archive.writestr(entry('META-INF/manifest.xml', ZIP_DEFLATED), MANIFEST)
        archive.writestr(entry('styles.xml', ZIP_DEFLATED), STYLES)
        archive.writestr(entry('content.xml', ZIP_DEFLATED), CONTENT)


if __name__ == '__main__':
    main()
