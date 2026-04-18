'use client'

import './globals.css'
import { Source_Sans_3 } from 'next/font/google'
import Sidebar from '@/components/Sidebar'
import { SidebarProvider } from '@/components/Sidebar/SidebarProvider'
import MainContent from '@/components/MainContent'
import { Toaster, toast } from 'sonner'
import "sonner/dist/styles.css"
import { useState, useEffect, useCallback } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { TooltipProvider } from '@/components/ui/tooltip'
import { RecordingStateProvider } from '@/contexts/RecordingStateContext'
import { OllamaDownloadProvider } from '@/contexts/OllamaDownloadContext'
import { TranscriptProvider } from '@/contexts/TranscriptContext'
import { ConfigProvider } from '@/contexts/ConfigContext'
import { OnboardingProvider } from '@/contexts/OnboardingContext'
import { OnboardingFlow } from '@/components/onboarding'
import { DownloadProgressToastProvider } from '@/components/shared/DownloadProgressToast'
import { RecordingPostProcessingProvider } from '@/contexts/RecordingPostProcessingProvider'

const sourceSans3 = Source_Sans_3({
  subsets: ['latin'],
  weight: ['400', '500', '600', '700'],
  variable: '--font-source-sans-3',
})

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [showOnboarding, setShowOnboarding] = useState(false)

  useEffect(() => {
    invoke<{ completed: boolean } | null>('get_onboarding_status')
      .then((status) => {
        const isComplete = status?.completed ?? false
        if (!isComplete) {
          setShowOnboarding(true)
        }
      })
      .catch(() => {
        setShowOnboarding(true)
      })
  }, [])

  // Disable context menu in production
  useEffect(() => {
    if (process.env.NODE_ENV === 'production') {
      const handle = (e: MouseEvent) => e.preventDefault()
      document.addEventListener('contextmenu', handle)
      return () => document.removeEventListener('contextmenu', handle)
    }
  }, [])

  // Forward tray recording toggle to the recording page
  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined

    listen('request-recording-toggle', () => {
      if (showOnboarding) {
        toast.error("Please complete setup first", {
          description: "You need to finish onboarding before you can start recording."
        })
      } else {
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'))
      }
    }).then(fn => {
      if (cancelled) {
        fn() // already unmounted, unsubscribe immediately
      } else {
        unlisten = fn
      }
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [showOnboarding])

  const handleOnboardingComplete = useCallback(() => {
    setShowOnboarding(false)
    window.location.reload() // Full reload ensures all Tauri state is re-initialized after onboarding
  }, [])

  return (
    <html lang="en">
      <body className={`${sourceSans3.variable} font-sans antialiased`}>
        <RecordingStateProvider>
          <TranscriptProvider>
            <ConfigProvider>
              <OllamaDownloadProvider>
                <OnboardingProvider>
                  <SidebarProvider>
                    <TooltipProvider>
                      <RecordingPostProcessingProvider>
                        <DownloadProgressToastProvider />

                        {showOnboarding ? (
                          <OnboardingFlow onComplete={handleOnboardingComplete} />
                        ) : (
                          <div className="flex">
                            <Sidebar />
                            <MainContent>{children}</MainContent>
                          </div>
                        )}
                      </RecordingPostProcessingProvider>
                    </TooltipProvider>
                  </SidebarProvider>
                </OnboardingProvider>
              </OllamaDownloadProvider>
            </ConfigProvider>
          </TranscriptProvider>
        </RecordingStateProvider>

        <Toaster position="bottom-center" richColors closeButton />
      </body>
    </html>
  )
}
