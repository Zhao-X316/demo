"""Simplified navigation, scoped review, restart and explicit publication. Synthetic IPC only."""
from pathlib import Path
from playwright.sync_api import expect, sync_playwright
from test_exam_fixed_intake_shared_shell_ui import MOCK_SCRIPT
from test_exam_objective_review_tab_ui import OBJECTIVE_MOCK_SCRIPT

ARTIFACTS = Path('/tmp/jiaofu-pew-simplification-20260907')
EMPTY_MATERIALS = r"""
const baseMaterials=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=(cmd,args)=>{
 if(cmd==='k1_blueprint_options')return Promise.resolve({classes:[],knowledge_maps:[],curriculum_nodes:[],knowledge_nodes:[],question_types:[]});
 if(cmd==='k1_question_search')return Promise.resolve({items:[],total:0,boundary_note:'测试资料'});
 return baseMaterials(cmd,args);
};
"""
SCOPED_MOCK = OBJECTIVE_MOCK_SCRIPT + r"""
window.__workspaceCalls=[];
window.__objectiveRows=[objectiveRow({student_name:'本批同学',suggestion_id:9001,crop_path:null}),objectiveRow({attempt_id:5002,student_name:'另一批同学',suggestion_id:9002,crop_path:null})];
window.__objectiveAttempts=[objectiveAttempt({student_name:'本批同学',item_count:1,confirmed_count:0}),objectiveAttempt({attempt_id:5002,student_name:'另一批同学',item_count:1,confirmed_count:0})];
const tasks=()=>window.__objectiveAttempts.map(a=>({kind:'exam_attempt',sourceId:a.attempt_id,title:'同一份作业',classId:1,className:'演示班',studentName:a.student_name,status:a.attempt_state==='published'?'published':window.__objectiveRows.find(r=>r.attempt_id===a.attempt_id).current_suggestion_confirmed?'ready_to_publish':'needs_review',updatedAt:'2026-09-07',assessmentVersionId:100,attemptIds:[a.attempt_id],hasEvidence:true}));
const workspaceBase=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
 window.__workspaceCalls.push({cmd,args});
 if(cmd==='classes_list')return [{id:1,name:'演示班'},{id:2,name:'另一班'}];
 if(cmd==='workspace_tasks')return tasks();
 if(cmd==='workspace_exam_review')return {task:tasks().find(t=>t.sourceId===args.sourceId),objective:{rows:window.__objectiveRows,attempts:window.__objectiveAttempts.map(a=>({...a,confirmed_count:window.__objectiveRows.find(r=>r.attempt_id===a.attempt_id).current_suggestion_confirmed?1:0,can_publish:window.__objectiveRows.find(r=>r.attempt_id===a.attempt_id).current_suggestion_confirmed}))},subjective:{rows:[],attempts:[]},dictation:{rows:[],attempts:[]}};
 return workspaceBase(cmd,args);
};
"""
RESUME_MOCK=MOCK_SCRIPT+r"""
const savedTask={kind:'exam_batch',sourceId:101,title:'保存的答题卡',classId:1,className:'演示班',studentName:null,status:'needs_material',updatedAt:'2026-09-07',assessmentVersionId:12,attemptIds:[301],hasEvidence:false};
const resumeBase=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
 if(cmd==='workspace_tasks')return [savedTask];
 if(cmd==='workspace_exam_review')return {task:savedTask,objective:{rows:[],attempts:[]},subjective:{rows:[],attempts:[]},dictation:{rows:[],attempts:[]}};
 if(cmd==='exam_fixed_intake_resume')return {classId:1,assessmentVersionId:12,result:preparedResult({studentPaths:['/tmp/saved.jpg'],expectedPagesPerAttempt:1}),processedPageIds:[301],processingHistory:[{id:1,stage:'answer_sheet_template',status:'failed',message:'原图不清晰，请核对材料',updatedAt:'2026-09-07'}]};
 return resumeBase(cmd,args);
};
"""


def no_overflow(page, name):
    for width, height in [(1280,800),(980,680)]:
        page.set_viewport_size({'width':width,'height':height})
        page.wait_for_timeout(100)
        assert page.locator('.main').evaluate('(el)=>el.scrollWidth<=el.clientWidth+1'), f'{name} overflows at {width}'
        page.screenshot(path=str(ARTIFACTS/f'{name}-{width}.png'))


def main():
    with sync_playwright() as p:
        browser=p.chromium.launch()
        page=browser.new_page(viewport={'width':1280,'height':800})
        page.add_init_script(MOCK_SCRIPT+EMPTY_MATERIALS)
        page.goto('http://127.0.0.1:4173')
        nav=page.get_by_role('navigation',name='主要导航')
        expect(nav.get_by_role('button')).to_have_text(['工作台','资料','学生'])
        expect(page.get_by_role('heading',name='工作台',exact=True)).to_be_visible()
        expect(page.get_by_text('搜索 / 跳转',exact=True)).to_have_count(0)
        no_overflow(page,'workspace')
        page.get_by_role('button',name='新建批改',exact=True).click()
        page.get_by_role('button',name='作业',exact=True).click()
        page.get_by_role('button',name='选择试卷',exact=True).click()
        expect(page.get_by_text('已选 6 份',exact=True)).to_be_visible()
        page.get_by_role('button',name='← 返回工作台',exact=True).click()
        page.get_by_role('button',name='新建批改',exact=True).click()
        page.once('dialog',lambda dialog:dialog.dismiss())
        page.get_by_role('button',name='作业',exact=True).click()
        expect(page.get_by_role('heading',name='新建批改',exact=True)).to_be_visible()
        page.get_by_role('button',name='← 返回工作台',exact=True).click()
        page.get_by_role('button',name='继续刚才的操作',exact=True).click()
        expect(page.get_by_text('已选 6 份',exact=True)).to_be_visible()
        answer=page.get_by_placeholder('也可以在这里粘贴答案')
        answer.fill('尚未保存的答案草稿')
        nav.get_by_role('button',name='资料',exact=True).click()
        expect(page.get_by_role('button',name='找题与查重',exact=True)).to_have_class('tab active')
        no_overflow(page,'materials')
        nav.get_by_role('button',name='工作台',exact=True).click()
        expect(answer).to_have_value('尚未保存的答案草稿')
        page.get_by_role('button',name='← 返回工作台',exact=True).click()
        page.get_by_role('button',name='继续刚才的操作',exact=True).click()
        expect(answer).to_have_value('尚未保存的答案草稿')
        no_overflow(page,'intake')
        page.close()

        missing=browser.new_page()
        missing.add_init_script(MOCK_SCRIPT+r"""
          localStorage.setItem('jiaofu.selected-class.v1','2');
          const noClassBase=window.__TAURI_INTERNALS__.invoke;
          window.__TAURI_INTERNALS__.invoke=(cmd,args)=>cmd==='classes_list'?Promise.resolve([{id:1,name:'有作业班'},{id:2,name:'暂无作业班'}]):noClassBase(cmd,args);
        """)
        missing.goto('http://127.0.0.1:4173')
        expect(missing.get_by_role('combobox',name='工作台班级')).to_have_value('2')
        missing.get_by_role('button',name='新建批改',exact=True).click()
        missing.get_by_role('button',name='作业',exact=True).click()
        expect(missing.get_by_text('当前班级还没有可批改的作业',exact=True)).to_be_visible()
        expect(missing.get_by_role('button',name='选择试卷',exact=True)).to_have_count(0)
        missing.close()

        page=browser.new_page(viewport={'width':1280,'height':800})
        page.add_init_script(SCOPED_MOCK)
        page.goto('http://127.0.0.1:4173')
        page.locator('.workspace-task').filter(has_text='本批同学').get_by_role('button').click()
        review=page.get_by_role('region',name='本次任务核对')
        expect(review.get_by_text('本批同学',exact=True)).to_be_visible()
        expect(review.get_by_text('另一批同学',exact=True)).to_have_count(0)
        expect(review.get_by_role('combobox')).to_have_count(0)
        expect(review.get_by_text('整卷发布',exact=False)).to_have_count(0)
        assert not page.evaluate('window.__objectiveCalls.length')
        page.reload()
        expect(review.get_by_text('本批同学',exact=True)).to_be_visible()
        assert not page.evaluate('window.__objectiveCalls.length'), 'restoring must not confirm or publish'
        page.get_by_role('button',name='← 返回工作台',exact=True).click()
        page.get_by_role('combobox',name='工作台班级').select_option('2')
        page.get_by_role('button',name='继续刚才的操作',exact=True).click()
        expect(page.locator('.task-scope')).to_contain_text('演示班')
        expect(review.get_by_text('本批同学',exact=True)).to_be_visible()
        no_overflow(page,'task-review')
        review.get_by_role('button',name='接受本条建议',exact=True).click()
        page.get_by_role('button',name='4 结果',exact=True).click()
        expect(page.get_by_role('button',name='发布这份结果',exact=True)).to_be_visible()
        expect(page.get_by_role('region',name='本次任务结果').get_by_text('另一批同学',exact=True)).to_have_count(0)
        assert not page.evaluate("window.__objectiveCalls.some(c=>c.cmd==='exam_objective_publish_attempt')")
        page.once('dialog',lambda dialog:dialog.dismiss())
        page.get_by_role('button',name='发布这份结果',exact=True).click()
        assert not page.evaluate("window.__objectiveCalls.some(c=>c.cmd==='exam_objective_publish_attempt')")
        page.once('dialog',lambda dialog:dialog.accept())
        page.get_by_role('button',name='发布这份结果',exact=True).click()
        expect(page.get_by_role('region',name='本次任务结果').get_by_text('已发布',exact=False)).to_be_visible()
        assert page.evaluate("window.__objectiveCalls.filter(c=>c.cmd==='exam_objective_publish_attempt').map(c=>c.args.attemptId)")==[5001]
        no_overflow(page,'task-results')
        page.close()

        page=browser.new_page()
        page.add_init_script(RESUME_MOCK)
        page.goto('http://127.0.0.1:4173?material=answer_sheet')
        page.locator('.workspace-task').get_by_role('button').click()
        expect(page.get_by_text('已恢复保存的处理记录',exact=True)).to_be_visible()
        expect(page.get_by_text('原图不清晰，请核对材料',exact=False)).to_be_visible()
        page.reload()
        expect(page.get_by_text('已恢复保存的处理记录',exact=True)).to_be_visible()
        page.wait_for_timeout(250)
        assert not page.evaluate("window.__fixedCalls.some(c=>c.cmd==='exam_answer_sheet_process_page'||c.cmd==='exam_fixed_intake_prepare')"), 'read-only reopen must not replay processing'
        no_overflow(page,'task-resume')
        page.close()
        browser.close()
    print('PASS navigation, draft retention, exact task scope, restart, explicit publication and both desktop sizes')


if __name__=='__main__':
    main()
