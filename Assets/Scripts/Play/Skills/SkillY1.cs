using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillY1 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject MyLineObj;
    GameObject MyLine;
    public float maxdistance = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    public float speed;
    public float damage;
    public float maxtime;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillY1()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            if (MyLine != null)
                MyLine.GetComponent<DestroyScript>().Destroyself();
            MyLine = Instantiate(MyLineObj, gameObject.transform.position, Quaternion.identity);
            MyLine.GetComponent<BlueLineScript>().sender = GetComponent<Rigidbody2D>();
            MyLine.GetComponent<BlueLineScript>().SetBSC(speed, damage, maxtime);
            GetComponent<DoSkill>().Fire = Skill;
        }
    }
    
    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        Fix64 mdf = (Fix64)maxdistance;
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        Fix64 rdf = skilldirection.Length();
        if (rdf > mdf)
            rdf = mdf;
        if (rdf <= (Fix64)0.51)
            return;
        Fix64Vector2 realplace = singplace + skilldirection.normalized() * rdf;
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        Vector2 rpv2 = realplace.ToV2();
        if (Physics2D.OverlapPoint(rpv2))
        {
            Collider2D hit = Physics2D.OverlapPoint(rpv2);
            if (hit.GetComponent<HPScript>() != null)
            {
                MyLine.GetComponent<BlueLineScript>().BlueLineWorking(hit.GetComponent<Rigidbody2D>());
                return;
            }
        }
        MyLine.GetComponent<BlueLineScript>().BlueLineMissed(rpv2);
    }

    void SkillY1SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }
}
