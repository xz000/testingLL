using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class SkillsLink : MonoBehaviour
{
    public GameObject mySoldier;
    public GameObject BottomPanel;
    public SkillCode? KeyCSkill = SkillCode.SkillC3;
    public SkillCode? KeyDSkill = SkillCode.TestSkillLightning;
    public SkillCode? KeyESkill = SkillCode.SkillE1;
    public SkillCode? KeyFSkill = SkillCode.TestSkill03;
    public SkillCode? KeyGSkill = SkillCode.TestSkill01;
    public SkillCode? KeyRSkill = SkillCode.TestSkill02;
    public SkillCode? KeyTSkill = SkillCode.TestSkillLeech;
    public SkillCode? KeyYSkill = SkillCode.SkillY1;
    public Sender sds;
    SkillData lsd;
    float TimeCount = 0;
    bool SettingSkillLevels;

    public void linktome(GameObject go)
    {
        mySoldier = go;
        selfset();
        BottomPanel.SetActive(true);
    }

    public void selfset()
    {
        gameObject.GetComponent<SetSkillC>().SetC();
        gameObject.GetComponent<SetSkillD>().SetD();
        gameObject.GetComponent<SetSkillE>().SetE();
        gameObject.GetComponent<SetSkillF>().SetF();
        gameObject.GetComponent<SetSkillG>().SetG();
        gameObject.GetComponent<SetSkillR>().SetR();
        gameObject.GetComponent<SetSkillT>().SetT();
        gameObject.GetComponent<SetSkillY>().SetY();
    }

    public void alphaset()
    {
        GetComponent<MainSkillMenu>().UnblockRaycasts();
        TimeCount = 0;
        SettingSkillLevels = true;
    }

    private void FixedUpdate()
    {
        if (SettingSkillLevels)
        {
            TimeCount += Time.fixedDeltaTime;
            if (TimeCount >= 1)
            {
                SettingSkillLevels = false;
                TimeCount = 0;
                selfset();
                Setlsd();
                sds.Sendlsd(lsd);
                betaset();
                GetComponent<MainSkillMenu>().CloseMainSkillMenu();
                Debug.Log("SL set and sent");
            }
        }
    }

    public void betaset()
    {
        sds.SetTempAndCheck(lsd.cNum, lsd.SLs);
        if (Sender.isTesting)
            sds.SetTempAndCheck(1 - lsd.cNum, lsd.SLs);
    }

    void Setlsd()
    {
        lsd = new SkillData();
        lsd.cNum = Sender.clientNum;
        int max = (int)SkillCode.SelfExplodeScript;
        lsd.SLs = new int[max];
        for (int i = 0; i < max; i++)
        {
            if (i == (int)KeyCSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyDSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyESkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyFSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyGSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyRSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyTSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
            if (i == (int)KeyYSkill)
            {
                lsd.SLs[i] = 1;
                continue;
            }
        }
    }
}
