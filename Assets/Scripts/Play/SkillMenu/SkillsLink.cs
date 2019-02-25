using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class SkillsLink : MonoBehaviour
{
    public GameObject mySoldier;
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

    public void linktome(GameObject go)
    {
        mySoldier = go;
        betaset();
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
        Setlsd();
    }

    public void alphaset()
    {
        sds.Sendlsd(lsd);
    }

    public void betaset()
    {
        sds.MyNS.GetComponent<ControllerScript>().SetSkillMem(lsd.cNum, lsd.SLs);
        selfset();//可以优化
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
