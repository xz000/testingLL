using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class SkillsLink : MonoBehaviour
{
    public GameObject mySoldier;
    public SkillCode KeyCSkill = SkillCode.SkillC3;
    public SkillCode KeyDSkill = SkillCode.TestSkillLightning;
    public SkillCode KeyESkill = SkillCode.SkillE1;
    public SkillCode KeyFSkill = SkillCode.TestSkill03;
    public SkillCode KeyGSkill = SkillCode.TestSkill01;
    public SkillCode KeyRSkill = SkillCode.TestSkill02;
    public SkillCode KeyTSkill = SkillCode.TestSkillLeech;
    public SkillCode KeyYSkill = SkillCode.SkillY1;

    public void linktome(GameObject go)
    {
        mySoldier = go;
        alphaset();
    }

    public void alphaset()
    {
        gameObject.GetComponent<SetSkillC>().SetC();
        //gameObject.GetComponent<SetSkillD>().SetD();
        //gameObject.GetComponent<SetSkillE>().SetE();
        //gameObject.GetComponent<SetSkillF>().SetF();
        //gameObject.GetComponent<SetSkillG>().SetG();
        //gameObject.GetComponent<SetSkillR>().SetR();
        //gameObject.GetComponent<SetSkillT>().SetT();
        //gameObject.GetComponent<SetSkillY>().SetY();
    }

    void SetSkillImages()
    {

    }
}
