import { createRouter, createWebHashHistory } from 'vue-router'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/library' },
    {
      path: '/library',
      name: 'library',
      component: () => import('@/pages/LibraryPage.vue'),
      meta: { title: 'Skill 库', eyebrow: '统一管理' },
    },
    {
      path: '/import',
      name: 'import',
      component: () => import('@/pages/ImportPage.vue'),
      meta: { title: '归集已有 Skill', eyebrow: '导入与迁移' },
    },
    {
      path: '/discover',
      name: 'discover',
      component: () => import('@/pages/DiscoveryPage.vue'),
      meta: { title: '发现与安装', eyebrow: '内容来源' },
    },
    {
      path: '/presets',
      name: 'presets',
      component: () => import('@/pages/PresetsPage.vue'),
      meta: { title: '预设', eyebrow: '组合与复用' },
    },
    {
      path: '/targets',
      name: 'targets',
      component: () => import('@/pages/TargetsPage.vue'),
      meta: { title: '分发目标', eyebrow: '工具与项目' },
    },
    {
      path: '/updates',
      name: 'updates',
      component: () => import('@/pages/UpdatesPage.vue'),
      meta: { title: '更新中心', eyebrow: '来源与版本' },
    },
    {
      path: '/tasks',
      name: 'tasks',
      component: () => import('@/pages/TasksPage.vue'),
      meta: { title: '任务与记录', eyebrow: '可追溯操作' },
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/pages/SettingsPage.vue'),
      meta: { title: '设置', eyebrow: 'SkillDock' },
    },
  ],
  scrollBehavior: (_to, _from, saved) => saved ?? { top: 0 },
})

export default router
