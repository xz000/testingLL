using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class TestSteamworks : MonoBehaviour
{
    public GameObject Menu01;
    public GameObject Menu02;

    private CGameID m_GameID;
    private int m_nTotalNumSR;
    private int m_nTotalNumDR;

    bool Initialized = false;
    private bool m_bStoreStats;
    // Did we get the stats from Steam?
    private bool m_bRequestedStats;
    private bool m_bStatsValid;

    protected Callback<UserStatsReceived_t> m_UserStatsReceived;
    protected Callback<UserStatsStored_t> m_UserStatsStored;

    private void Awake()
    {
        try
        {
            if (SteamAPI.RestartAppIfNecessary((AppId_t)908660))
            {
                Application.Quit();
                return;
            }
        }
        catch (System.DllNotFoundException e)
        {
            Debug.LogError("[Steamworks.NET] Could not load [lib]steam_api.dll/so/dylib. It's likely not in the correct location. Refer to the README for more details.\n" + e, this);
            Application.Quit();
            return;
        }
        //Debug.Log("Awaken");
    }

    // Use this for initialization
    void Start()
    {
        bool ib = SteamAPI.Init();
        if (ib)
            Debug.Log("Steam API init -- SUCCESS!");
        else
            Debug.Log("Steam API init -- failure ...");
        m_GameID = new CGameID(SteamUtils.GetAppID());
        m_UserStatsReceived = Callback<UserStatsReceived_t>.Create(OnUserStatsReceived);
        m_UserStatsStored = Callback<UserStatsStored_t>.Create(OnUserStatsStored);
        m_bRequestedStats = false;
        m_bStatsValid = false;
        Initialized = ib;
        Menu01.SetActive(true);
        Menu02.SetActive(false);
    }

    private void Update()
    {
        if (!Initialized)
            return;
        SteamAPI.RunCallbacks();

        if (!m_bRequestedStats)
        {
            // Is Steam Loaded? if no, can't get stats, done
            if (!Initialized)
            {
                m_bRequestedStats = true;
                return;
            }

            // If yes, request our stats
            bool bSuccess = SteamUserStats.RequestCurrentStats();

            // This function should only return false if we weren't logged in, and we already checked that.
            // But handle it being false again anyway, just ask again later.
            m_bRequestedStats = bSuccess;
        }

        if (!m_bStatsValid)
            return;

        //Store stats in the Steam database if necessary
        if (m_bStoreStats)
        {
            // set stats
            SteamUserStats.SetStat("stat_SR", m_nTotalNumSR);
            SteamUserStats.SetStat("stat_DR", m_nTotalNumDR);
            bool bSuccess = SteamUserStats.StoreStats();
            // If this failed, we never sent anything to the server, try
            // again later.
            m_bStoreStats = !bSuccess;
        }
    }

    private void OnUserStatsReceived(UserStatsReceived_t pCallback)
    {
        if (!Initialized)
            return;

        // we may get callbacks for other games' stats arriving, ignore them
        if ((ulong)m_GameID == pCallback.m_nGameID)
        {
            if (EResult.k_EResultOK == pCallback.m_eResult)
            {
                //Debug.Log("Received stats and achievements from Steam");
                // load stats
                SteamUserStats.GetStat("stat_SR", out m_nTotalNumSR);
                SteamUserStats.GetStat("stat_DR", out m_nTotalNumDR);

                m_bStatsValid = true;
            }
            else
            {
                Debug.Log("RequestStats - failed, " + pCallback.m_eResult);
            }
        }
    }

    private void OnUserStatsStored(UserStatsStored_t pCallback)
    {
        // we may get callbacks for other games' stats arriving, ignore them
        /*
        if ((ulong)m_GameID == pCallback.m_nGameID)
        {
            if (EResult.k_EResultOK == pCallback.m_eResult)
            {
                Debug.Log("StoreStats - success");
            }
            else if (EResult.k_EResultInvalidParam == pCallback.m_eResult)
            {
                // One or more stats we set broke a constraint. They've been reverted,
                // and we should re-iterate the values now to keep in sync.
                Debug.Log("StoreStats - some failed to validate");
                // Fake up a callback here so that we re-load the values.
                UserStatsReceived_t callback = new UserStatsReceived_t();
                callback.m_eResult = EResult.k_EResultOK;
                callback.m_nGameID = (ulong)m_GameID;
                OnUserStatsReceived(callback);
            }
            else
            {
                Debug.Log("StoreStats - failed, " + pCallback.m_eResult);
            }
        }
        */
    }

    public void GameEndResultSet(bool a)
    {
        if (a)
            m_nTotalNumSR++;
        else
            m_nTotalNumDR++;
        m_bStoreStats = true;
    }

    private void OnDestroy()
    {
        if (!Initialized)
            return;
        SteamAPI.Shutdown();
        Debug.Log("SteamAPI Shutdown");
    }
}